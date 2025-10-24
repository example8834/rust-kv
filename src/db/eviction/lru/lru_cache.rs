use std::{collections::{BinaryHeap, HashMap}, hash::Hash, sync::Arc};

use crate::{db::eviction::{lru::{lru_struct::LruMemoryCache, LruCache}, EvictionWriteOp, TtlEntry}, types::ValueEntry};




impl EvictionWriteOp for LruCache {
    fn write_op(binary_heap: &mut BinaryHeap<TtlEntry>,memory_cache: &mut LruMemoryCache, key: Arc<String>, value: &ValueEntry) {
        if let Some(expires_at) = value.expires_at {
            let ttl_entry = TtlEntry {
                expires_at,
                key: key.clone(),
            };
            binary_heap.push(ttl_entry);
        }
        memory_cache.put(key);
    }

    fn read_op(binary_heap: &mut BinaryHeap<TtlEntry>,memory_cache: &mut LruMemoryCache, key: Arc<String>) {
        memory_cache.read(key);
    }

    fn delete_op(binary_heap: &mut BinaryHeap<TtlEntry>,memory_cache: &mut LruMemoryCache, key: Arc<String>) {
        memory_cache.delete(key);
    }
    
}