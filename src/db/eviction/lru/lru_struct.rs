use std::{collections::HashMap, ptr::NonNull, sync::Arc};

use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::RwLock;

use crate::db::eviction::lru::lru_linklist::{LruList, Node};
const NUM_SHARDS: usize = 32; // 32 个分片

#[derive(Debug, Clone)]
struct LruNode {
    pub map_key: HashMap<Arc<String>, NonNull<Node>>,
    pub list: LruList,
}
#[derive(Default, Debug, Clone)]
pub struct LruMemoryCache {
    pub message: Vec<Arc<RwLock<LruNode>>>,
}

unsafe impl Send for LruNode {}
unsafe impl Sync for LruNode {}

impl LruMemoryCache {
    //自定义新算法 来确定key 应该落在哪个分片
    fn get_shard_index<K: Hash>(key: &K) -> usize {
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
            })));
        }
        Self { message }
    }

    //通过 专门的hash算法 获取在那个分片
    pub fn put(&mut self, key: Arc<String>) {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let mut shard = self.message[shard_index].blocking_write();
        //接下来就是在这个分片上操作
        let contaion = shard.map_key.contains_key(&key);
        if !contaion {
            let node_ptr = shard.list.push_back(key.clone());
            shard.map_key.insert(key, node_ptr);
        } else {
            //只能克隆 也应该克隆
            let node_ptr = shard.map_key.get(&key).unwrap().clone();
            shard.list.push_mid_back(node_ptr);
        }
    }

    pub fn delete(&mut self, key: Arc<String>) {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let mut shard = self.message[shard_index].blocking_write();
        //接下来就是在这个分片上操作
        if let Some(node_ptr) = shard.map_key.remove(&key) {
            shard.list.pop_node(node_ptr);
        }
    }

    pub fn read(&mut self, key: Arc<String>) {
        let shard_index = LruMemoryCache::get_shard_index(&key);
        let mut shard = self.message[shard_index].blocking_write();
        //接下来就是在这个分片上操作
        let node_ptr = shard.map_key.get(&key).unwrap().clone();
        shard.list.push_mid_back(node_ptr);
    }
}
