use std::{collections::BinaryHeap, sync::Arc};

use crate::{db::eviction::{EvictionWriteOp, TtlEntry}, types::ValueEntry};

pub(crate) struct LruCache;


impl EvictionWriteOp for LruCache {
    fn write_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>, value: &ValueEntry) {
        // LRU 写操作的具体实现
    }

    fn read_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>) {
        // LRU 读操作的具体实现
    }

    fn delete_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: &str) {
        // LRU 删除操作的具体实现
    }
    
}