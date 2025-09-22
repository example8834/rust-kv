use std::collections::HashMap;

use crate::db::{LockType, LockedDb, ValueEntry};

// 假设 LockedDb 包含 db 的引用和 guard
impl<'a> LockedDb<'a> {

    // 一个通用的、处理【写操作】的模板方法
    pub fn execute_write_op<F>(&mut self, key: String,value: ValueEntry, mut operation: F)
    where
        // ✅ 核心改动：我们要求一个【FnMut】闭包
        //    它接收一个对底层 HashMap 的【可变引用】
        F: FnMut(String,ValueEntry ,&mut HashMap<String, ValueEntry>),
    {
        // 1. 从 guard 中获取底层的、可变的 HashMap
        let map = if let LockType::Write(ref mut guard) = self.guard {
            // guard 是 RwLockWriteGuard，它 DerefMut 到 HashMap
             guard 
        } else {
            panic!("Attempted to write with a read lock");
        };

        
        // 2. 执行由调用者传入的、具体的数据操作
        operation(key,value,map);

        // log::info!(...);             // 比如，自动写日志
    }

}