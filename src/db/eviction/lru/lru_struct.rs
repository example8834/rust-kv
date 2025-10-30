use std::{
    collections::HashMap,
    ptr::NonNull,
    sync::{Arc, atomic::AtomicUsize},
};

use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::RwLock;

use crate::{
    config::GLOBAL_MEMORY,
    db::eviction::lru::lru_linklist::{LruList, Node},
    types::ValueEntry,
};
pub const NUM_SHARDS: usize = 32; // 32 个分片

#[derive(Debug)]
pub struct LruNode {
    pub map_key: HashMap<Arc<String>, MetaPointers>,
    pub list: LruList,
    pub sample_keys: Vec<Arc<String>>, // O(1) 采样数组
    pub db_store: HashMap<Arc<String>, ValueEntry>,
    pub approx_memory: AtomicUsize, // 它自己分片的账 记录具体的内存大小
    pub global_memory: Arc<AtomicUsize>, // 它也持有 Arc 指针
}

#[derive(Debug, Clone)]
// 你的新 Value 结构
pub struct MetaPointers {
    pub lru_node: NonNull<Node>, // 指向 LRU 链表节点
    pub sample_idx: usize,       // 指向 Vec<Key> 的索引
}

#[derive(Default, Debug, Clone)]
pub struct LruMemoryCache {
    pub message: Vec<Arc<RwLock<LruNode>>>,
}

unsafe impl Send for LruNode {}
unsafe impl Sync for LruNode {}

impl LruMemoryCache {
    //自定义新算法 来确定key 应该落在哪个分片
    pub fn get_shard_index<K: Hash>(key: &K) -> usize {
        // 1. 创建 FxHasher
        let mut hasher = FxHasher::default(); // 变化在这里！

        // 2. 喂 key
        key.hash(&mut hasher);

        // 3. 拿结果
        let hash_value = hasher.finish();

        // 4. 取模
        (hash_value as usize) % NUM_SHARDS
    }
    pub fn new() -> Self {
        let mut message = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            message.push(Arc::new(RwLock::new(LruNode {
                map_key: HashMap::new(),
                list: LruList::new(),
                sample_keys: Vec::new(),
                db_store: HashMap::new(),
                approx_memory: AtomicUsize::new(0).into(),
                global_memory: Arc::clone(&GLOBAL_MEMORY),
            })));
        }
        Self { message }
    }

    //通过 专门的hash算法 获取在那个分片
    pub async fn put(&self, key: Arc<String>) -> tokio::sync::RwLockWriteGuard<'_, LruNode> {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let mut shard = self.message[shard_index].write().await;
        //接下来就是在这个分片上操作
        let contaion = shard.map_key.contains_key(&key);
        if !contaion {
            let node_ptr = shard.list.push_back(key.clone());

            let index = shard.sample_keys.len();
            shard.sample_keys.push(key.clone());

            shard.map_key.insert(
                key,
                MetaPointers {
                    lru_node: node_ptr,
                    sample_idx: index,
                },
            );
        } else {
            //只能克隆 也应该克隆 这里只修改链表
            let node_ptr = shard.map_key.get(&key).unwrap().clone();
            shard.list.push_mid_back(node_ptr.lru_node);
        }
        shard
    }

    /**
     * 由于没有把锁加入到内部 就是类似unsafe 一样
     * 需要自己控制锁 防止重入 
     * 外部的具体方法都加入到锁上 就可以不用二次调用锁了
     */
    pub async fn pop<'a>(
        mut shard: tokio::sync::RwLockWriteGuard<'a, LruNode>,
        key: Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'a, LruNode> {
        // 1. 从 master_map 删除，拿到元数据
        if let Some(node_ptr) = shard.map_key.remove(&key) {
            // 2. 从 LRU 链表删除
            shard.list.pop_node(node_ptr.lru_node);

            // 3. 从 Vec 中删除 (O(1))
            let idx_to_remove = node_ptr.sample_idx;
            shard.sample_keys.swap_remove(idx_to_remove);

            let moved_key_cloned = shard.sample_keys.get(idx_to_remove).cloned();

            // 5. 现在 `moved_key_cloned` 是一个拥有的值，它不借用 shard
            if let Some(key) = moved_key_cloned {
                // 6. 我们可以【安全地】对 shard 进行【可变借用】
                if let Some(moved_meta) = shard.map_key.get_mut(&key) {
                    moved_meta.sample_idx = idx_to_remove;
                }
            }
        }
        shard
    }


    pub async fn read(&self, key: Arc<String>) -> tokio::sync::RwLockReadGuard<'_, LruNode> {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let mut shard = self.message[shard_index].write().await;
        // 必须用 if let 或 match 来安全地处理
        if let Some(meta_ptr) = shard.map_key.get(&key) {
            //接下来就是在这个分片上操作
            let node_ptr = meta_ptr.lru_node;
            shard.list.push_mid_back(node_ptr);
        }
        shard.downgrade()
    }

    pub async fn get_lock_write(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, LruNode> {
        self.put(key.clone()).await
    }

    pub async fn get_lock_read(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockReadGuard<'_, LruNode> {
        self.read(key.clone()).await
    }

    pub async fn get_lock_delete(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, LruNode> {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let shard = self.message[shard_index].write().await;
        Self::pop(shard,key.clone()).await
    }
}
