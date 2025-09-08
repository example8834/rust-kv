use bytes::Bytes;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
};
// 确保有这行
use tokio::sync::{ MutexGuard, RwLock};

use crate::error::KvError;

// 这个枚举代表了 Redis 支持的各种数据类型
// 1. 让 Value 枚举本身可以 Clone
#[derive(Clone, Debug)]
pub enum Value {
    String(Bytes),
    List(VecDeque<Bytes>),
    Hash(HashMap<String, Bytes>),
    Set(HashSet<Bytes>),
    // 以后还可以添加 ZSet (Sorted Set) 等
}
// 1. 让 Value 枚举本身可以 Clone
#[derive(Clone, Debug)]
pub struct ValueEntry {
    pub data: Value,
    pub expires_at: Option<u64>, // u64 用来存过期时间点的时间戳
}

// 我们的核心存储结构
type DbStore = HashMap<String, ValueEntry>;

// 把 Arc<Mutex<...>> 封装到一个新结构里，这是个好习惯
#[derive(Clone, Default)]
pub struct Db {
    store: Arc<RwLock<DbStore>>,
}
impl Db {
    // 提供一个公共的构造函数
    pub fn new() -> Self {
        // ... 你的初始化逻辑，比如启动后台任务 ...
        Self::default()
    }

    // 提供一个公共的、异步的 `get` 方法
    pub async fn get(&self, key: &str) -> Option<ValueEntry> {
        let store = self.store.read().await;
        // 这里的逻辑可能还包含检查 key 是否过期
        store.get(key).cloned() // 假设 ValueEntry 是 Clone 的
    }

    // 提供一个公共的、异步的 `set` 方法
    pub async fn set<F>(
        &self,
        key: String,
        value: ValueEntry,
        hook: Option<F>,
    ) -> Result<(), KvError>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = Result<(), KvError>> + Send>>,
    {
        let mut store = self.store.write().await;
        store.insert(key, value);
        if let Some(fun) = hook {
            match fun().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e.into());
                }
            };
        }
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(),KvError>{
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
    }
}
