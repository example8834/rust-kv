use std::{
    collections::{BinaryHeap, HashMap},
    hash::Hash,
    sync::Arc,
};

use crate::{
    db::eviction::{
        EvictionWriteOp, TtlEntry,
        lru::{LruCache, lru_struct::LruMemoryCache},
    },
    types::ValueEntry,
};
