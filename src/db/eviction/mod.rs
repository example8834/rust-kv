use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use crate::types::{LockType, LockedDb, ValueEntry};

pub mod lfu_cache;
pub mod lru_cache;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TtlEntry {
    expires_at: u64,
    key: Arc<String>,
}
#[derive(Default, Debug)]
pub struct EvictionManager {
    pub(crate) ttl_heap: BinaryHeap<TtlEntry>,
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
impl<'a> LockedDb<'a> {
    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_write_op<F>(&mut self, key: String, value: ValueEntry, mut operation: F)
    where
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnMut(String, ValueEntry, &mut HashMap<String, ValueEntry>),
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<String, ValueEntry>> =
            if let LockType::Write(ref mut guard) = self.guard {
                // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
                guard
            } else {
                panic!("Attempted to write with a read lock");
            };

        // 2. 执行由调用者传入的、具体的数据操作
        operation(key, value, map);
        
        // log::info!(...);             // 比如，自动写日志
    }

    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_read_op<F>(&self, key: String, operation: F) -> Option<ValueEntry>
    where
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnOnce(String, &HashMap<String, ValueEntry>) -> Option<ValueEntry>,
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map: &tokio::sync::RwLockReadGuard<'_, HashMap<String, ValueEntry>> =
            if let LockType::Read(ref guard) = self.guard {
                // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
                guard
            } else {
                panic!("Attempted to write with a read lock");
            };

        // 2. 执行由调用者传入的、具体的数据操作
        operation(key, map)
        // log::info!(...);             // 比如，自动写日志
    }
}
