use bytes::Bytes;
use itoa::Buffer;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::{Arc, OnceLock},
    time::Instant,
};
pub mod string;
pub mod eviction;
// 确保有这行
use tokio::sync::RwLock;

use crate::error::KvError;

#[derive(Clone, Debug, PartialEq, Eq, Hash)] // 需要派生 Hash 和 Eq 才能用于 HashSet
pub enum Element {
    String(Bytes),
    Int(i64),
}

// 第二步：修改顶层的 Value 枚举，让集合类型使用 Element
#[derive(Clone, Debug)]
pub enum Value {
    // 对于简单的 K-V，值就是一个 Element
    Simple(Element),

    // 集合类型包含的是 Element 的集合
    List(VecDeque<Element>),
    Hash(HashMap<String, Element>), // Hash 的 value 也是 Element
    Set(HashSet<Element>),
}
// 1. 让 Value 枚举本身可以 Clone
#[derive(Clone, Debug)]
pub struct ValueEntry {
    pub data: Value,
    pub expires_at: Option<u64>, // u64 用来存过期时间点的时间戳
    pub eviction_metadata: u64,      // 32位记录最近访问时间戳 后32 记录访问次数
}

// 这是一个新的、公开的结构体
// 它的生命周期 'a 被绑定到它持有的 MutexGuard
pub struct LockedDb<'a> {
    // 关键：它持有锁的守卫，但这个字段是私有的！
    // 外界无法通过 LockedDb 直接访问 guard.data
    pub guard: LockType<'a>,
}

pub enum LockType<'a> {
    Write(tokio::sync::RwLockWriteGuard<'a, HashMap<String, ValueEntry>>),
    Read(tokio::sync::RwLockReadGuard<'a, HashMap<String, ValueEntry>>),
}

// 我们的核心存储结构
type DbStore = HashMap<String, ValueEntry>;

// 把 Arc<Mutex<...>> 封装到一个新结构里，这是个好习惯
#[derive(Clone, Default)]
pub struct Db {
    store: Arc<RwLock<DbStore>>,
}
//其实理论上操作需要绑定数据 并且内聚的情况下。理论上操作db 就应该核心的方法收拾有db提供 外部不应该耦合到db内部
//db内部就应该提供方法 如果外部执行指令 都需要调用db 也不错 就是如果无法接偶 db提供方法尽量底层 让外部拼接
//也是不错的 如果就需要和db底层耦合比较多的情况下 那大部分方法 涉及底层操作 需要db的封装比较多 可以分开不同文件来增加可读性
impl Db {
    // 提供一个公共的构造函数
    pub fn new() -> Self {
        // ... 你的初始化逻辑，比如启动后台任务 ...
        Self::default()
    }



    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_write(&self) -> LockedDb<'_> {
        //这里是创建 出来的这个db
        LockedDb {
            guard: LockType::Write(self.store.write().await),
        }
    }

    pub async fn lock_read(&self) -> LockedDb<'_> {
        //这里是创建 出来的这个db
        LockedDb {
            guard: LockType::Read(self.store.read().await),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), KvError> {
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
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
