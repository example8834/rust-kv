use std::{
    clone,
    collections::{BinaryHeap, HashMap, binary_heap},
    sync::{Arc, atomic::AtomicUsize},
};

use crate::{
    config::{CONFIG, EvictionType},
    db::{
        eviction::lru::{LruCache, lru_struct::LruNode},
    },
    types::ValueEntry,
};
use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::{RwLock, RwLockWriteGuard};

pub mod lfu;
pub mod lru;

pub const NUM_SHARDS: usize = 32; // 32 个分片

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TtlEntry {
    expires_at: u64,
    key: Arc<String>,
}

pub struct MemoryCacheNode {
    pub db_store: HashMap<Arc<String>, ValueEntry>,
    pub approx_memory: AtomicUsize, // 它自己分片的账 记录具体的内存大小
    pub evicition: Box<dyn EvictionPolicy>,
}

impl MemoryCacheNode {
    pub fn new(config_type: &EvictionType) -> Self {
        // 【你说的 "match 一下" + "new 一下"】
        let policy_instance: Box<dyn EvictionPolicy> = match config_type {
            EvictionType::LRU => {
                // "对应结构new一下"
                Box::new(LruNode::new())
            }
            EvictionType::LFU => {
                // "对应结构new一下"
                // Box::new(LfuPolicy::new())
                todo!()
            }
        };
        MemoryCacheNode {
            db_store: HashMap::new(),
            approx_memory: AtomicUsize::new(0),
            evicition: policy_instance,
        }
    }
}

#[derive(Default, Clone)]
pub struct MemoryCache {
    pub message: Vec<Arc<RwLock<MemoryCacheNode>>>,
}

impl MemoryCache {
    pub fn new(config_type: &EvictionType) -> Self {
        // 1. 先拿到一个“空”的 self (store 是个空 Vec)
        let mut local_vec: Vec<Arc<RwLock<MemoryCacheNode>>> = Vec::with_capacity(NUM_SHARDS);

        //默认创建 16 个数据库
        for _ in 0..16 {
            // 假设 LruMemoryCache 也有一个 new()
            local_vec.push(Arc::new(RwLock::new(MemoryCacheNode::new(config_type))));
        }
        MemoryCache { message: local_vec }
    }

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

    pub async fn get_lock_write(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> =
            self.message[shard_index].write().await;
        shard
    }

    pub async fn get_lock_read(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> =
            self.message[shard_index].write().await;
        shard
    }

    pub async fn get_lock_delete(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockWriteGuard<'_, MemoryCacheNode> =
            self.message[shard_index].write().await;
        shard
    }
}

pub trait EvictionPolicy: Send + Sync {
    // 当写入时，策略需要做什么？
    fn on_write(&mut self, key: Arc<String>);
    // 当读取时，策略需要做什么？
    fn on_read(&mut self, key: &Arc<String>);
    // 当删除时...
    fn on_delete(&mut self, key: Arc<String>);
    // “获取里面的 key 数组” -> 抽象成 -> “给我一个随机 key”
    fn get_random_sample_key(&self) -> Option<Arc<String>>;
    // 挑选一个删除者
    fn pop_victim(&mut self) -> Option<Arc<String>>;
}
