use std::{collections::{BinaryHeap, HashMap}, hash::Hash, sync::Arc};

use crate::{db::eviction::{lru::LruCache, EvictionWriteOp, TtlEntry}, types::ValueEntry};




impl EvictionWriteOp for LruCache {
    fn write_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>, value: &ValueEntry) {
        if let Some(expires_at) = value.expires_at {
            let ttl_entry = TtlEntry {
                expires_at,
                key,
            };
            binary_heap.push(ttl_entry);
        }
    }

    fn read_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: Arc<String>) {

    }

    fn delete_op(binary_heap: &mut BinaryHeap<TtlEntry>, key: &str) {
        
    }
    
}