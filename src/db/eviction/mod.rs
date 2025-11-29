use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::core_time::get_cached_time_ms;
use crate::{config::EvictionType, db::eviction::lru::lru_struct::LruNode, types::ValueEntry};
use async_trait::async_trait;
use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::{Mutex, MutexGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
pub mod eviction_alo;
pub mod lfu;
pub mod lru;

pub const NUM_SHARDS: usize = 64; // 64 个分片
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TtlEntry {
    expires_at: u64,
    key: Arc<String>,
}

pub struct MemoryCacheNode {
    pub db_store: HashMap<Arc<String>, ValueEntry>,
    pub approx_memory: AtomicUsize, // 它自己分片的账 记录具体的内存大小
    pub evicition: Mutex<Box<dyn EvictionPolicy>>,
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

#[async_trait]
impl KvOperator for LuaCacheNode {
    async fn insert(&mut self, key: Arc<String>, value: ValueEntry) {
        let size_before = match self.select(&key).await {
            Some(entry) => entry.data_size,
            None => 0,
        };
        //插入修改类别的 都是覆盖 如果没有就插入
        let memory_differ = value.data_size as isize - size_before as isize;
        self.differ_map.insert(key, ChangeOp::Update(value));
        self.local_memory_diff += memory_differ;
    }

    async fn select(&mut self, key: &Arc<String>) -> Option<&ValueEntry> {
        match self.differ_map.get(key) {
            Some(change) => match change {
                ChangeOp::Update(value_entry) => Some(value_entry),
                ChangeOp::Delete => None,
            },
            None => self.db_store.select(key).await,
        }
    }

    //说明一下 这个usize 转 isize 就是在小于800万TB都是没问题  位数足够大 一般不会超过这个的感觉
    async fn delete(&mut self, key: &Arc<String>) {
        let size_before = match self.select(&key).await {
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

#[async_trait]
impl Transactional for LuaCacheNode {
    async fn commit(&mut self) {
        for (key, change) in self.differ_map.drain() {
            match change {
                ChangeOp::Update(value_entry) => {
                    self.db_store.insert(key, value_entry).await;
                }
                ChangeOp::Delete => {
                    self.db_store.delete(&key).await;
                }
            }
        }
    }
}

impl<'a> LuaCacheNode {
    fn new(db_store: DirectCacheNode) -> Self {
        LuaCacheNode {
            db_store,
            differ_map: HashMap::new(),
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
            evicition: Mutex::new(policy_instance),
        }
    }

    /* 没办法 这个锁封装的问题 暂时留一个接口 后续补充调整
     */
    fn get_memory_usage(&self) -> usize {
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

#[async_trait]
impl KvOperator for DirectCacheNode {
    async fn insert(&mut self, key: Arc<String>, value: ValueEntry) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                //首先标记出触发淘汰策略
                rw_lock_write_guard
                    .evicition
                    .lock()
                    .await
                    .on_write(key.clone());
                let size_before = match rw_lock_write_guard.db_store.get(&key) {
                    Some(entry) => entry.data_size,
                    None => 0,
                };

                //值差异
                let memory_differ = value.data_size as isize - size_before as isize;

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
            DirectCacheNode::Readguard(_rw_lock_read_guard) => {}
        }
    }

    async fn delete(&mut self, key: &Arc<String>) {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                if let Some(value) = rw_lock_write_guard.db_store.remove(key) {
                    //触发淘汰策略
                    rw_lock_write_guard
                        .evicition
                        .lock()
                        .await
                        .on_delete(key.clone());
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
    async fn select(&mut self, key: &Arc<String>) -> Option<&ValueEntry> {
        match self {
            // 1. 【语法修正】这里不要写 ref mut，直接写变量名 guard
            // 因为 self 是 &mut，guard 自动就是可变引用
            // guard 的类型其实是： &mut OwnedRwLockWriteGuard<MemoryCacheNode>
            DirectCacheNode::Writeguard(guard) => {
                // let eviction = guard.evicition.lock().await;
                // 1. 【关键修正】剥两层壳！
                // 第一个 * ：解开 &mut 引用，拿到 Guard 智能指针
                // 第二个 * ：触发 Guard 的 Deref，拿到内部的 MemoryCacheNode
                // &mut    ：重新获取 Node 的可变引用
                let node = &mut **guard;

                // 2. 【简单粗暴】手动拿字段
                // 这样写，编译器 100% 知道 store 和 eviction 是分开的
                // 绝对不会报 "borrowed more than once"
                let store = &mut node.db_store;
                let eviction = &mut node.evicition;

                // 3. 先更新 LRU (操作 eviction)
                eviction.lock().await.on_read(key);

                // 4. 【第一查】只拿 bool 标记
                // 这一步只借用 store 一瞬间，用完立刻释放
                let should_remove = if let Some(v) = store.get(key) {
                    if let Some(t) = v.expires_at {
                        get_cached_time_ms() > t
                    } else {
                        false
                    }
                } else {
                    return None;
                };

                // 5. 【第二查】根据标记行动
                // 此时 store 是完全自由的
                if should_remove {
                    store.remove(key);
                    None
                } else {
                    // 没过期，重新获取并返回
                    store.get(key)
                }
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => {
                // 1. 拿引用 (0 开销)
                let node = &**rw_lock_read_guard;
                let store = &node.db_store;
                let eviction = &node.evicition; // 这里是 Mutex<Box<dyn Policy>>

                // 2. 更新 LRU (内部可变性，微小开销)
                // 这里的 await 只是为了拿那个极短的 Mutex，不会阻塞太久
                eviction.lock().await.on_read(key);

                // 3. 查数据
                if let Some(value) = store.get(key) {
                    // 4. 检查过期
                    if let Some(expire_time) = value.expires_at {
                        if get_cached_time_ms() > expire_time {
                            // 【惰性删除策略】
                            // 发现过期 -> 既然只读锁删不掉 -> 直接返回 None
                            // 此时在业务层看来，key 已经不存在了
                            return None;
                        }
                    }

                    // 5. 命中返回
                    // 记得返回 Clone 的值 (ValueEntry)，不要返回引用
                    return Some(value);
                }

                None
            }
        }
    }

    fn as_lock_owner(self: Box<Self>) -> Option<Box<dyn LockOwner>> {
        Some(self)
    }
}

#[async_trait]
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

    async fn get_eviction_policy(&self) -> Option<MutexGuard<'_, Box<dyn EvictionPolicy>>> {
        match self {
            DirectCacheNode::Writeguard(rw_lock_write_guard) => {
                let lock: tokio::sync::MutexGuard<'_, Box<dyn EvictionPolicy + 'static>> =
                    rw_lock_write_guard.evicition.lock().await;
                Some(lock)
            }
            DirectCacheNode::Readguard(rw_lock_read_guard) => None,
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
#[async_trait]
pub trait KvOperator: Send + Sync {
    async fn insert(&mut self, key: Arc<String>, value: ValueEntry);
    async fn select(&mut self, key: &Arc<String>) -> Option<&ValueEntry>;
    async fn delete(&mut self, key: &Arc<String>);

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
#[async_trait]
pub trait Transactional: KvOperator {
    async fn commit(&mut self);
}

//数据库最基本的三个操作
#[async_trait]
pub trait LockOwner: KvOperator {
    // 1. 暴露内存大小 (AtomicUsize 通常只返回数值 usize)
    fn get_memory_usage(&self) -> usize;

    // 2. 暴露驱逐策略 (返回引用 &dyn，而不是 Box)
    async fn get_eviction_policy(&self) -> Option<MutexGuard<'_, Box<dyn EvictionPolicy>>>;

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

    pub async fn get_lock_write(&self, key: &Arc<String>) -> Box<dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard = self.message[shard_index].clone().write_owned().await;
        Box::new(DirectCacheNode::Writeguard(shard))
    }

    // Lua 调度层调用这个
    // 注意：这里传入了 differ_map
    pub async fn lock_write_lua(&self, key: &Arc<String>) -> (Box<dyn KvOperator>, usize) {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard = self.message[shard_index].clone().write_owned().await;
        (
            Box::new(LuaCacheNode::new(DirectCacheNode::Writeguard(shard))),
            shard_index,
        )
    }

    //这个是lua 真正用的
    pub async fn get_lua_lock_write_shard_index(&self, shard_index: usize) -> Box<dyn KvOperator> {
        let shard = self.message[shard_index].clone().write_owned().await;
        Box::new(LuaCacheNode::new(DirectCacheNode::Writeguard(shard)))
    }

    pub async fn get_lock_write_shard_index(&self, shard_index: usize) -> Box<dyn KvOperator> {
        let shard = self.message[shard_index].clone().write_owned().await;
        Box::new(DirectCacheNode::Writeguard(shard))
    }

    pub async fn get_lock_read(&self, key: &Arc<String>) -> Box<dyn KvOperator> {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard = self.message[shard_index].clone().read_owned().await;
        Box::new(DirectCacheNode::Readguard(shard))
    }

    pub async fn get_lock_read_shard_index(&self, shard_index: usize) -> Box<dyn KvOperator> {
        let shard = self.message[shard_index].clone().read_owned().await;
        Box::new(DirectCacheNode::Readguard(shard))
    }

    // Lua 调度层调用这个
    // 注意：这里传入了 differ_map
    pub async fn lock_read_lua(&self, key: &Arc<String>) -> (Box<dyn KvOperator>, usize) {
        let shard_index = MemoryCache::get_shard_index(&key);
        let shard = self.message[shard_index].clone().read_owned().await;
        (
            Box::new(LuaCacheNode::new(DirectCacheNode::Readguard(shard))),
            shard_index,
        )
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
