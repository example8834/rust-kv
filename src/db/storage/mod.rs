use bytes::Bytes;
use itoa::Buffer;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Instant,
};
pub mod string;

// 确保有这行
use tokio::sync::RwLock;

use crate::{error::KvError, types::{ ConnectionState, Storage, ValueEntry}};


// 1. 让 Value 枚举本身可以 Clone

// 这是一个新的、公开的结构体
// 它的生命周期 'a 被绑定到它持有的 MutexGuard
pub struct LockedDb<'a> {
    // 关键：它持有锁的守卫，但这个字段是私有的！
    // 外界无法通过 LockedDb 直接访问 guard.data
    pub guard: LockType<'a>,
}

pub enum LockType<'a> {
    Write(tokio::sync::RwLockWriteGuard<'a, HashMap<Arc<String>, ValueEntry>>),
    Read(tokio::sync::RwLockReadGuard<'a, HashMap<Arc<String>, ValueEntry>>),
}



//其实理论上操作需要绑定数据 并且内聚的情况下。理论上操作db 就应该核心的方法收拾有db提供 外部不应该耦合到db内部
//db内部就应该提供方法 如果外部执行指令 都需要调用db 也不错 就是如果无法接偶 db提供方法尽量底层 让外部拼接
//也是不错的 如果就需要和db底层耦合比较多的情况下 那大部分方法 涉及底层操作 需要db的封装比较多 可以分开不同文件来增加可读性
impl Storage {
    // 提供一个公共的构造函数
    pub fn new() -> Self {
        // ... 你的初始化逻辑，比如启动后台任务 ...
        Self::default()
    }



    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_write(&self,connect_state:& mut ConnectionState) -> LockedDb<'_> {
        //这里是创建 出来的这个db
        LockedDb {
            guard: LockType::Write(self.store.get(connect_state.selected_db).unwrap().write().await),
        }
    }

    pub async fn lock_read(&self,connect_state:& mut ConnectionState) -> LockedDb<'_> {
        //这里是创建 出来的这个db
        LockedDb {
            guard: LockType::Read(self.store.get(connect_state.selected_db).unwrap().read().await),
        }
    }

}

// 一个直接从 Bytes 高效解析 i64 的函数
pub fn bytes_to_i64_fast(b: &Bytes) -> Option<i64> {
    // 顯式標註 result 變量的類型
    // 直接告訴 parse 函數，你想解析成 i64
    let result = lexical_core::parse::<i64>(b);
    result.ok()
}

//高效的int 转byte 方法
pub fn parse_int_from_bytes(i: i64) -> Bytes {
    let mut buffer = Buffer::new();

    // 2. 将数字格式化到缓冲区中，返回一个指向缓冲区内容的 &str
    let printed_str = buffer.format(i);

    // 3. 从结果切片创建 Bytes (这里有一次复制，但避免了堆分配)
    Bytes::copy_from_slice(printed_str.as_bytes())
}
