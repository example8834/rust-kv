use std::{collections::{HashMap, HashSet, VecDeque}, sync::Arc};

use bytes::Bytes;
use tokio::sync::RwLock;

//结构共享的模块
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

#[derive(Clone, Debug)]
pub struct ValueEntry {
    pub data: Value,
    pub expires_at: Option<u64>, // u64 用来存过期时间点的时间戳
    pub eviction_metadata: u64,      // 32位记录最近访问时间戳 后32 记录访问次数
}
// 我们的核心存储结构
type DbStore = HashMap<String, ValueEntry>;

// 把 Arc<Mutex<...>> 封装到一个新结构里，这是个好习惯
// 这个数组
#[derive(Clone, Default)]
pub struct Storage {
    pub(crate) store: Arc<RwLock<DbStore>>,
}