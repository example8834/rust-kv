use std::{
    clone, collections::{binary_heap, BinaryHeap, HashMap}, sync::Arc
};

use tokio::sync::RwLock;

use crate::{
    config::{EvictionType, CONFIG},
    db::{ eviction::lru::{lru_struct::LruMemoryCache, LruCache}, LockType, LockedDb},
    types::ValueEntry,
};

pub mod lfu;
pub mod lru;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TtlEntry {
    expires_at: u64,
    key: Arc<String>,
}

#[derive(Default, Debug,Clone)]
pub struct EvictionManager {
    pub memory_eviction: LruMemoryCache,
}

impl EvictionManager {
    // 这里可以添加 EvictionManager 的方法，比如处理过期键
    // 提供一个公共的构造函数
    pub fn new() -> Self {
        // ... 你的初始化逻辑，比如启动后台任务 ...
        Self::default()
    }
}

trait EvictionWriteOp {
    fn write_op(memory_cache: &mut LruMemoryCache, key: Arc<String>, value: &ValueEntry);
    fn read_op(memory_cache: &mut LruMemoryCache, key: Arc<String>);
    fn delete_op(memory_cache: &mut LruMemoryCache, key: Arc<String>);
}

// 假设 LockedDb 包含 db 的引用和 guard
// 这里根据上下文来获取 具体那个淘汰类型

