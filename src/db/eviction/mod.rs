use std::{
    collections::{BinaryHeap, HashMap, binary_heap},
    sync::Arc,
};

use tokio::sync::RwLock;
use tracing_subscriber::fmt::writer;

use crate::{
    config::{EvictionType, CONFIG},
    db::{ eviction::lru::LruCache, LockType, LockedDb},
    types::ValueEntry,
};

mod lfu;
mod lru;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TtlEntry {
    expires_at: u64,
    key: Arc<String>,
}

#[derive(Default, Debug, Clone)]
pub struct EvictionManager {
    pub ttl_heap: Arc<RwLock<BinaryHeap<TtlEntry>>>,
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
    fn write_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>, value: &ValueEntry);
    fn read_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>);
    fn delete_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: &str);
}

// 假设 LockedDb 包含 db 的引用和 guard
// 这里根据上下文来获取 具体那个淘汰类型
impl EvictionManager {
    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_write_op(&mut self, key: Arc<String>, value: &ValueEntry) {
        let mut binary_heap = self.ttl_heap.blocking_write();
        match CONFIG.eviction_type {
            EvictionType::LRU => {
                LruCache::write_op(&mut binary_heap, key, value);
            }
            EvictionType::LFU => {}
        }
    }

    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_read_op(&self, key: Arc<String>) {
        let mut binary_heap = self.ttl_heap.blocking_write();
        match CONFIG.eviction_type {
            EvictionType::LRU => {
                LruCache::read_op(&mut binary_heap, key);
            }
            EvictionType::LFU => {}
        }
    }

    pub fn execute_delete(&mut self, key: &str) {
        let mut binary_heap = self.ttl_heap.blocking_write();
        match CONFIG.eviction_type {
            EvictionType::LRU => {
                LruCache::delete_op(&mut binary_heap, key);
            }
            EvictionType::LFU => {}
        }   
    }
}
