use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use tokio::sync::RwLock;

use crate::{db::{LockType, LockedDb}, types::ValueEntry};

mod lfu_cache;
mod lru_cache;

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

// 假设 LockedDb 包含 db 的引用和 guard
impl EvictionManager{
    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_write_op(&mut self, key: Arc<String>, value: &ValueEntry)    {
        let a = self.ttl_heap.blocking_read();
        // log::info!(...);             // 比如，自动写日志
    }

    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_read_op(&self, key: Arc<String>)
    {

    }


    pub fn execute_delete(&mut self, key: &str) 
    {
    }
}
