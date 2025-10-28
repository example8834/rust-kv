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
    // 关键！这个 Entry 内部的数据(不含key)总共占了多少内存
    pub data_size: usize,
}
