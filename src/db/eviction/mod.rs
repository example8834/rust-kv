use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use tokio::sync::RwLock;

use crate::{db::storage::{LockType, LockedDb}, types::{ ValueEntry}};

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
impl<'a> LockedDb<'a> {
    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_write_op<F>(&mut self, key: Arc<String>, value: ValueEntry,manager:&mut EvictionManager, mut operation: F)
    where
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnMut(Arc<String>, ValueEntry,&mut EvictionManager,&mut HashMap<Arc<String>, ValueEntry>),
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<Arc<String>, ValueEntry>> =
            if let LockType::Write(ref mut guard) = self.guard {
                // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
                guard
            } else {
                panic!("Attempted to write with a read lock");
            };

        // 2. 执行由调用者传入的、具体的数据操作
        operation(key, value,manager, map);
        
        // log::info!(...);             // 比如，自动写日志
    }

    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_read_op<F>(&self, key: Arc<String>,manager:&mut EvictionManager, operation: F) -> Option<ValueEntry>
    where
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnOnce(Arc<String>,&mut EvictionManager, &HashMap<Arc<String>, ValueEntry>) -> Option<ValueEntry>,
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map: &tokio::sync::RwLockReadGuard<'_, HashMap<Arc<String>, ValueEntry>> =
            if let LockType::Read(ref guard) = self.guard {
                // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
                guard
            } else {
                panic!("Attempted to write with a read lock");
            };

        // 2. 执行由调用者传入的、具体的数据操作
        operation(key,manager, map)
        // log::info!(...);             // 比如，自动写日志
    }


    pub fn execute_delete<F>(&mut self,manager:&mut EvictionManager, key: &str, mut operation: F) 
    where
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnMut(&str,&mut EvictionManager, &mut HashMap<Arc<String>, ValueEntry>),
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map: &mut tokio::sync::RwLockWriteGuard<'_, HashMap<Arc<String>, ValueEntry>> =
            if let LockType::Write(ref mut guard) = self.guard {
                // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
                guard
            } else {
                panic!("Attempted to write with a read lock");
            };

        // 2. 执行由调用者传入的、具体的数据操作
        operation(key,manager, map);
        // log::info!(...);             // 比如，自动写日志
    }
}
