use std::collections::{HashMap, HashSet, VecDeque};

use bytes::Bytes;

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

impl Value {
    // 只计算分配在【堆 (Heap)】上的额外内存
    // 不包含 Value 枚举本身在栈上的大小
    pub fn heap_memory_size(&self) -> usize {
        match self {
            // Simple 里的 Bytes 是存堆上的，Element 本身在栈上
            // 所以这里只加 bytes 的 len
            Value::Simple(el) => el.heap_size(),

            Value::List(deque) => {
                // VecDeque 本身有一定的堆预分配空间 (capacity)
                // 这里简化计算：元素总堆大小 + 容器基础堆开销
                let elements_heap: usize = deque.iter().map(|e| e.heap_size()).sum();
                // deque.capacity() * size_of::<Element>() 是它在堆上占用的连续内存
                let container_heap = deque.capacity() * std::mem::size_of::<Element>();
                elements_heap + container_heap
            },

            Value::Hash(map) => {
                let elements_heap: usize = map.iter()
                    .map(|(k, v)| k.len() + v.heap_size()) // Key string 也在堆上
                    .sum();
                // HashMap 的 bucket 数组在堆上
                let container_heap = map.capacity() * (std::mem::size_of::<String>() + std::mem::size_of::<Element>());
                elements_heap + container_heap
            },

            Value::Set(set) => {
                let elements_heap: usize = set.iter().map(|e| e.heap_size()).sum();
                let container_heap = set.capacity() * std::mem::size_of::<Element>();
                elements_heap + container_heap
            }
        }
    }
}

impl Element {
    pub fn heap_size(&self) -> usize {
        match self {
            // Bytes 的实际数据在堆上
            Element::String(b) => b.len(),
            // Int 是纯栈数据，没有堆开销
            Element::Int(_) => 0,
        }
    }
}

impl ValueEntry {
    pub fn new(data: Value, expires_at: Option<u64>) -> Self {
        // 1. 计算结构体本身的“物理体积” (Stack Overhead)
        //    这包含了 data(enum本身), expires_at, data_size 字段
        let struct_size = std::mem::size_of::<Self>();

        // 2. 计算数据持有的“额外体积” (Heap Memory)
        let heap_size = data.heap_memory_size();

        // 3. 总大小 = 壳 + 肉
        let total_size = struct_size + heap_size;

        Self {
            data,
            expires_at,
            data_size: total_size, // 这回准了！
        }
    }

    pub fn get_size(&self) -> usize {
        self.data_size
    }
}