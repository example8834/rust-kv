use bytes::Bytes;
use itoa::Buffer;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Instant,
};
mod eviction;
mod generic;
mod hash;
mod list;
mod string;

// 确保有这行
use tokio::sync::RwLock;

use crate::{
    context::{CONN_STATE, ConnectionState},
    db::eviction::lru::lru_struct::{LruMemoryCache, LruNode},
    error::KvError,
    types::ValueEntry,
};

pub(crate) use eviction::EvictionManager;

// 3. 定义并公开那个唯一的、组合好的顶层结构
#[derive(Clone)]
pub struct Db {
    pub store: Storage,
}
impl Db {
    pub fn new() -> Self {
        Self {
            store: Storage::new(),
        }
    }
}
// 1. 让 Value 枚举本身可以 Clone

// 这是一个新的、公开的结构体
// 它的生命周期 'a 被绑定到它持有的 MutexGuard
pub struct LockedDb<'a> {
    // 关键：它持有锁的守卫，但这个字段是私有的！
    // 外界无法通过 LockedDb 直接访问 guard.data
    pub guard: LockType<'a>,
}

//这个的粒度就是基本的粒度
pub enum LockType<'a> {
    Write(tokio::sync::RwLockWriteGuard<'a, LruNode>),
    Read(tokio::sync::RwLockReadGuard<'a, LruNode>),
}

// 这个数组
#[derive(Clone, Default)]
pub struct Storage {
    pub(crate) store: Vec<Arc<LruMemoryCache>>,
}

//其实理论上操作需要绑定数据 并且内聚的情况下。理论上操作db 就应该核心的方法收拾有db提供 外部不应该耦合到db内部
//db内部就应该提供方法 如果外部执行指令 都需要调用db 也不错 就是如果无法接偶 db提供方法尽量底层 让外部拼接
//也是不错的 如果就需要和db底层耦合比较多的情况下 那大部分方法 涉及底层操作 需要db的封装比较多 可以分开不同文件来增加可读性
impl Storage {
    // 提供一个公共的构造函数
    pub fn new() -> Self {
        // 1. 先拿到一个“空”的 self (store 是个空 Vec)
        let mut this = Self::default();

        // 2. 【你的自定义逻辑】
        // 比如，像 Redis 一样，默认创建 16 个数据库
        for _ in 0..16 {
            // 假设 LruMemoryCache 也有一个 new()
            this.store.push(Arc::new(LruMemoryCache::new()));
        }

        // 4. 返回“初始化好”的 self
        this
    }

    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_write(&self, key: &Arc<String>) -> LockedDb<'_> {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb {
            guard: LockType::Write(self.store.get(select_db).unwrap().get_lock_write(key).await),
        }
    }

    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_read(&self, key: &Arc<String>) -> LockedDb<'_> {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb {
            guard: LockType::Read(self.store.get(select_db).unwrap().get_lock_read(key).await),
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
