use std::{
    clone,
    collections::{BinaryHeap, HashMap, binary_heap},
    sync::{Arc, atomic::{AtomicUsize, Ordering}},
};

use crate::{
    config::{CONFIG, EvictionType},
    db::eviction::lru::{LruCache, lru_struct::LruNode},
    types::ValueEntry,
};
use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::{RwLock, RwLockWriteGuard};

pub mod eviction_alo;
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

//lua 变更级数据源模拟
// 定义一个包装类型
pub enum ChangeOp {
    Update(ValueEntry), 
    Delete,             
}

pub struct LuaCacheNode {
    // 读源
    pub db_store: HashMap<Arc<String>, ValueEntry>,
    // 写缓冲：Value 变成 ChangeOp
    pub differ_map: HashMap<Arc<String>, ChangeOp>, 
}

impl KvOperator for LuaCacheNode {
    fn insert(&mut self, key: Arc<String>, value: ValueEntry, memory_differ: usize) {
        self.differ_map.insert(key, ChangeOp::Update(value));
    }

    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry> {
        self.db_store.get(key)
    }

    fn delete(&mut self, key: &Arc<String>, memory_differ: usize) {
        self.differ_map.insert(key.clone(), ChangeOp::Delete);
    }
    
}

impl LuaCacheNode {
    fn new(db_store:HashMap<Arc<String>, ValueEntry>,differ_map:HashMap<Arc<String>, ChangeOp>) ->Self{
        LuaCacheNode { db_store , differ_map }
    }
}

impl MemoryCacheNode {
    fn new(config_type: &EvictionType) -> Self {
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

impl KvOperator for MemoryCacheNode {
    fn insert(&mut self, key: Arc<String>, value: ValueEntry,memory_differ:usize) {
        self.db_store.insert(key, value);
        self.approx_memory.fetch_add(memory_differ, Ordering::Relaxed);
    }

    fn delete(&mut self, key: &Arc<String>,memory_differ:usize) {
        self.db_store.remove(key);
        self.approx_memory.fetch_sub(memory_differ, Ordering::Relaxed);
    }

    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry>{
    self.db_store.get(key)
    }

    fn as_lock_owner(&self) -> Option<&dyn LockOwner> {
        // 因为我自己实现了 LockOwner，所以我可以把自己转成 Trait Object 返回
        Some(self)
    }

    // 如果你需要修改大接口状态，也可以加这个
    fn as_lock_owner_mut(&mut self) -> Option<&mut dyn LockOwner> {
        Some(self)
    }

}

// 这是一个纯净的接口，只管数据读写
impl LockOwner for MemoryCacheNode {
    
    
    fn get_memory_usage(&self) -> usize {
        // AtomicUsize 读取需要指定 Ordering
        // Relaxed 性能最高，通常用于统计足够了
        self.approx_memory.load(Ordering::Relaxed)
    }

    fn get_eviction_policy(&mut self) -> &mut dyn EvictionPolicy {
        // self.evicition 是 Box<dyn EvictionPolicy>
        // 我们用 as_ref() 或者直接解引用，返回里面的 &dyn 对象
        self.evicition.as_mut()
    }
    fn add_memory(&self, size: usize) {
        self.approx_memory.fetch_add(size, Ordering::Relaxed);
    }

    fn sub_memory(&self, size: usize) {
        self.approx_memory.fetch_sub(size, Ordering::Relaxed);
    }
}

#[derive(Default, Clone)]
pub struct MemoryCache {
    pub message: Vec<Arc<RwLock<dyn KvOperator>>>,
}



// 这是一个纯净的接口，只管数据读写
pub trait KvOperator : Send + Sync{
    fn insert(&mut self, key: Arc<String>, value: ValueEntry, memory_differ: usize);
    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry>;
    fn delete(&mut self, key: &Arc<String>, memory_differ: usize);

    fn as_lock_owner(&self) -> Option<&dyn LockOwner> {
        None // 默认情况下，我不是
    }
    
    // 如果你需要修改大接口状态，也可以加这个
    fn as_lock_owner_mut(&mut self) -> Option<&mut dyn LockOwner> {
        None
    }
}

//数据库最基本的三个操作
pub trait LockOwner : KvOperator {
    // 1. 暴露内存大小 (AtomicUsize 通常只返回数值 usize)
    fn get_memory_usage(&self) -> usize;

    // 2. 暴露驱逐策略 (返回引用 &dyn，而不是 Box)
    fn get_eviction_policy(&mut self) -> &mut dyn EvictionPolicy;

    // 修改内存记账 (封装成行为更好，不要直接暴露 Atomic)
    fn add_memory(&self, size: usize);
    fn sub_memory(&self, size: usize);
}

impl MemoryCache {
    pub fn new(config_type: &EvictionType) -> Self {
        // 1. 先拿到一个“空”的 self (store 是个空 Vec)
        let mut local_vec: Vec<Arc<RwLock<dyn KvOperator>>> = Vec::with_capacity(NUM_SHARDS);

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
    ) -> tokio::sync::RwLockWriteGuard<'_, dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockWriteGuard<'_, dyn KvOperator> =
            self.message[shard_index].write().await;
        shard
    }

    pub async fn get_lock_read(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockReadGuard<'_, dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockReadGuard<'_, dyn KvOperator> =
            self.message[shard_index].read().await;
        shard
    }

    pub async fn get_lock_delete(
        &self,
        key: &Arc<String>,
    ) -> tokio::sync::RwLockWriteGuard<'_, dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard: tokio::sync::RwLockWriteGuard<'_, dyn KvOperator> =
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
