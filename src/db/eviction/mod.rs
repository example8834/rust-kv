use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    config::EvictionType,
    db::eviction::lru::lru_struct::LruNode,
    types::ValueEntry,
};
use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
use crate::core_time::get_cached_time_ms;
pub mod eviction_alo;
pub mod lfu;
pub mod lru;

pub const NUM_SHARDS: usize = 32; // 32 个分片
pub const NUM_DBS: usize = 32; // 32 个分片
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

// 包装器代理
pub struct LuaCacheNode {
    pub db_store: DirectCacheNode,
    pub differ_map: HashMap<Arc<String>, ChangeOp>,
    pub local_memory_diff: isize,
}

impl<'a> KvOperator for LuaCacheNode {
    fn insert(&mut self, key: Arc<String>, value: ValueEntry) {
        let size_before= match self.select(&key){
                Some(entry) => entry.data_size,
                None => 0,
        };
        //插入修改类别的 都是覆盖 如果没有就插入 
        let memory_differ = value.data_size as isize  - size_before as isize;
        self.differ_map.insert(key, ChangeOp::Update(value));
        self.local_memory_diff += memory_differ;
    }

    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry> {
        match self.differ_map.get(key) {
            Some(change) => {
                match  change{
                    ChangeOp::Update(value_entry) => {
                        Some(value_entry)
                    },
                    ChangeOp::Delete => {
                        None
                    },
                }
            },
            None => {
                self.db_store.select(key)
            },
        }
    }

    //说明一下 这个usize 转 isize 就是在小于800万TB都是没问题  位数足够大 一般不会超过这个的感觉
    fn delete(&mut self, key: &Arc<String>) {
        let size_before= match self.select(&key){
                Some(entry) => entry.data_size,
                None => 0,
        };
        self.differ_map.insert(key.clone(), ChangeOp::Delete);
        self.local_memory_diff -= size_before as isize;
    }
  // 事务缓冲方法
    fn as_transactional(self: Box<Self>) -> Option<Box<dyn Transactional>> {
        Some(self)
    }
}

impl Transactional for LuaCacheNode {
    fn commit(&mut self) {
        for (key,change) in self.differ_map.drain() {
            match change {
                ChangeOp::Update(value_entry) => {
                    self.db_store.insert(key, value_entry);
                },
                ChangeOp::Delete => {
                    self.db_store.delete(&key);
                },
            }
        }
    }
}


impl<'a> LuaCacheNode {
    fn new(
        db_store: DirectCacheNode,
    ) -> Self {
        LuaCacheNode {
            db_store,
            differ_map:HashMap::new(),
            local_memory_diff: 0,
        }
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

    /* 没办法 这个锁封装的问题 暂时留一个接口 后续补充调整
     */
    fn get_memory_usage(&self) ->usize{
        self.approx_memory.load(Ordering::Relaxed)
    }
}

// 场景 A: 普通模式的包装器
// 它只负责持有锁，操作直接透传给底层
pub enum DirectCacheNode {
    // 这里持有 map 过的锁
    Writeguard(OwnedRwLockWriteGuard<MemoryCacheNode>),
    Readguard(OwnedRwLockReadGuard<MemoryCacheNode>),
}

impl KvOperator for DirectCacheNode {
    fn insert(&mut self, key: Arc<String>, value: ValueEntry) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {

            let size_before= match rw_lock_write_guard.db_store.get(&key){
                Some(entry) => entry.data_size,
                None => 0,
            };
            //值差异
            let memory_differ = value.data_size as isize  - size_before as isize;

            //插入数值的时候 消耗掉这个
                rw_lock_write_guard.db_store.insert(key, value);

                // 2. 根据差值的正负，决定是加还是减
                if memory_differ > 0 {
                    // 内存增加了：转成 usize 加进去
                    rw_lock_write_guard
                        .approx_memory
                        .fetch_add(memory_differ as usize, Ordering::Relaxed);
                } else if memory_differ < 0 {
                    // 内存减少了：取绝对值（变成正数），然后减出去
                    // (-memory_differ) 就变成了正数，比如 -50 变成 50
                    rw_lock_write_guard
                        .approx_memory
                        .fetch_sub((-memory_differ) as usize, Ordering::Relaxed);
                }
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {}
        }
    }

    fn delete(&mut self, key: &Arc<String>) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                if let Some(value) = rw_lock_write_guard.db_store.remove(key){
                    rw_lock_write_guard
                    .approx_memory
                    .fetch_sub(value.data_size, Ordering::Relaxed);
                }
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {}
        }
    }

    /*
    读写在内核代理层就完成
     */
    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry> {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                if let Some(value) = rw_lock_write_guard.db_store.get(key) {
                    let time_expires = value.expires_at;
                    if let Some(expire_time) = time_expires {
                        if get_cached_time_ms() > expire_time {
                            return None;
                        }
                    }
                    return Some(value);
                }
                return None;
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                if let Some(value) = rw_lock_read_guard.db_store.get(key) {
                    let time_expires = value.expires_at;
                    if let Some(expire_time) = time_expires {
                        if get_cached_time_ms() > expire_time {
                            return None;
                        }
                    }
                    return Some(value);
                }
                return None;
            },
        }
    }

    fn as_lock_owner(self: Box<Self>) -> Option<Box<dyn LockOwner>> {
        Some(self)
    }
}

impl LockOwner for DirectCacheNode {
    fn get_memory_usage(&self) -> usize {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                rw_lock_write_guard.approx_memory.load(Ordering::Relaxed)
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                rw_lock_read_guard.approx_memory.load(Ordering::Relaxed)
            }
        }
    }

    fn get_mut_eviction_policy(&mut self) -> Option<&mut dyn EvictionPolicy> {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                Some(rw_lock_write_guard.evicition.as_mut())
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => None,
        }
    }
    fn get_ref_eviction_policy(&mut self) -> Option<&dyn EvictionPolicy> {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => None,
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                Some(rw_lock_read_guard.evicition.as_ref())
            }
        }
    }

    fn add_memory(&self, size: usize) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                rw_lock_write_guard
                    .approx_memory
                    .fetch_add(size, Ordering::Relaxed);
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                rw_lock_read_guard
                    .approx_memory
                    .fetch_add(size, Ordering::Relaxed);
            }
        }
    }

    fn sub_memory(&self, size: usize) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                rw_lock_write_guard
                    .approx_memory
                    .fetch_sub(size, Ordering::Relaxed);
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                rw_lock_read_guard
                    .approx_memory
                    .fetch_sub(size, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct MemoryCache {
    pub message: Vec<Arc<RwLock<MemoryCacheNode>>>,
}

// 这是一个纯净的接口，只管数据读写
pub trait KvOperator: Send + Sync {
    fn insert(&mut self, key: Arc<String>, value: ValueEntry);
    fn select(&self, key: &Arc<String>) -> Option<&ValueEntry>;
    fn delete(&mut self, key: &Arc<String>);

    // 【核心修改】
    // 不要用 into_inner(self)，要用引用！
    // 意思是：给我看看你肚子里的真锁
    // fn get_direct_node(&mut self) -> &mut DirectCacheNode;

    fn as_lock_owner(self: Box<Self>) -> Option<Box<dyn LockOwner>> {
        None // 默认情况下，我不是
    }
    // 事务缓冲方法
    fn as_transactional(self: Box<Self>) -> Option<Box<dyn Transactional>> {
        None
    }
}
pub trait Transactional:KvOperator {
    fn commit(&mut self);
}

//数据库最基本的三个操作
pub trait LockOwner: KvOperator {
    // 1. 暴露内存大小 (AtomicUsize 通常只返回数值 usize)
    fn get_memory_usage(&self) -> usize;

    // 2. 暴露驱逐策略 (返回引用 &dyn，而不是 Box)
    fn get_mut_eviction_policy(&mut self) -> Option<&mut dyn EvictionPolicy>;

    fn get_ref_eviction_policy(&mut self) -> Option<&dyn EvictionPolicy>;

    // 修改内存记账 (封装成行为更好，不要直接暴露 Atomic)
    fn add_memory(&self, size: usize);
    fn sub_memory(&self, size: usize);
}

impl MemoryCache {
    pub fn new(config_type: &EvictionType) -> Self {
        // 1. 先拿到一个“空”的 self (store 是个空 Vec)
        let mut local_vec: Vec<Arc<RwLock<MemoryCacheNode>>> = Vec::with_capacity(NUM_SHARDS);

        //默认创建 32 个数据分片
        for _ in 0..NUM_SHARDS {
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

    pub async fn get_lock_write(& self, key: &Arc<String>) -> Box<dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard =
            self.message[shard_index].clone().write_owned().await;
        Box::new(DirectCacheNode::Writeguard(shard))
    }

    // Lua 调度层调用这个
    // 注意：这里传入了 differ_map
    pub async fn lock_write_lua(
        &self,
        key: &Arc<String>,
    ) -> (Box<dyn KvOperator>,usize) {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard =
            self.message[shard_index].clone().write_owned().await;
        (Box::new(LuaCacheNode::new(
            DirectCacheNode::Writeguard(shard),
        )),shard_index)
    }

    pub async fn get_lock_write_shard_index(& self, shard_index: usize) -> Box<dyn KvOperator > {
        let shard =
            self.message[shard_index].clone().write_owned().await;
        Box::new(DirectCacheNode::Writeguard(shard))
    }

    pub async fn get_lock_read(&self, key: &Arc<String>) -> Box<dyn KvOperator > {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard=
            self.message[shard_index].clone().read_owned().await;
        Box::new(DirectCacheNode::Readguard(shard))
    }

    pub async fn get_lock_read_shard_index(& self, shard_index: usize) -> Box<dyn KvOperator> {
        let shard =
            self.message[shard_index].clone().read_owned().await;
        Box::new(DirectCacheNode::Readguard(shard))
    }

    // Lua 调度层调用这个
    // 注意：这里传入了 differ_map
    pub async fn lock_read_lua(
        & self,
        key: &Arc<String>,
    ) -> (Box<dyn KvOperator>,usize) {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard =
            self.message[shard_index].clone().read_owned().await;
        (Box::new(LuaCacheNode::new(
            DirectCacheNode::Readguard(shard)
        )),shard_index)
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
