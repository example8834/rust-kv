use bytes::Bytes;
use itoa::Buffer;
use std::sync::Arc;
pub mod eviction;
mod generic;
mod hash;
mod list;
mod string;

// 确保有这行

use crate::{
    config::EvictionType,
    context::CONN_STATE,
    db::eviction::{
        KvOperator, LockOwner, MemoryCache,
    },
};

// 3. 定义并公开那个唯一的、组合好的顶层结构
#[derive(Clone)]
pub struct Db {
    pub store: Storage,
}
impl Db {
    pub fn new(config_type: &EvictionType) -> Self {
        Self {
            store: Storage::new(config_type),
        }
    }
}
// 1. 让 Value 枚举本身可以 Clone
//这个的粒度就是基本的粒度
pub enum LockedDb {
    Write(Box<dyn KvOperator>),
    Read(Box<dyn KvOperator>),
}

// 这个数组 最外层的arc 是为了共享
#[derive(Clone, Default)]
pub struct Storage {
    pub(crate) store: Arc<Vec<Arc<MemoryCache>>>,
}

//内核通用接口

//其实理论上操作需要绑定数据 并且内聚的情况下。理论上操作db 就应该核心的方法收拾有db提供 外部不应该耦合到db内部
//db内部就应该提供方法 如果外部执行指令 都需要调用db 也不错 就是如果无法接偶 db提供方法尽量底层 让外部拼接
//也是不错的 如果就需要和db底层耦合比较多的情况下 那大部分方法 涉及底层操作 需要db的封装比较多 可以分开不同文件来增加可读性
impl Storage {
    // 提供一个公共的构造函数
    pub fn new(config_type: &EvictionType) -> Self {
        // 1. 先拿到一个“空”的 self (store 是个空 Vec)
        let mut local_vec: Vec<Arc<MemoryCache>> = Vec::with_capacity(16);

        //默认创建 16 个数据库
        for _ in 0..16 {
            // 假设 LruMemoryCache 也有一个 new()
            local_vec.push(Arc::new(MemoryCache::new(config_type)));
        }

        // 4. 返回“初始化好”的 self
        Storage {
            store: Arc::new(local_vec),
        }
    }

    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_write(&self, key: &Arc<String>) -> LockedDb {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb::Write(self.store.get(select_db).unwrap().get_lock_write(key).await)
    }

    // lock() 方法现在返回这个新的 LockedDb 守卫，而不是原始的 MutexGuard
    pub async fn lock_read(&self, key: &Arc<String>) -> LockedDb {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb::Read(self.store.get(select_db).unwrap().get_lock_read(key).await)
    }

    /*
    下面俩方法是lua 的方法
     */
    pub async fn lock_write_lua<'a>(&'a self, shard_index: usize) -> LockedDb {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb::Write(self.store.get(select_db).unwrap().get_lock_write_shard_index(shard_index).await)
    }

    pub async fn lock_read_lua<'a>(&'a self, shard_index: usize) -> LockedDb  {
        let select_db = CONN_STATE.with(|state| state.selected_db);
        LockedDb::Read(self.store.get(select_db).unwrap().get_lock_read_shard_index(shard_index).await)
    }

    // 修改后（正确）：
    pub async fn get_lock_write(
        self,
        db_index: usize,
        shard_index: usize,
    ) -> Box<dyn LockOwner> {
        self.store
            .get(db_index)
            .unwrap()
            .get_lock_write_shard_index(shard_index)
            .await
            .as_lock_owner()
            .unwrap()
    }

    // 修改后（正确）：
    pub async fn get_lock_read(
        &self,
        db_index: usize,
        shard_index: usize,
    ) -> Box<dyn LockOwner>  {
        self.store
            .get(db_index)
            .unwrap().clone().get_lock_read_shard_index(shard_index)
            .await
            .as_lock_owner().unwrap()
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
