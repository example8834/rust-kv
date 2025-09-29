mod eviction;
mod storage;

pub(crate) use eviction::EvictionManager;

use crate::types::Storage;

// 3. 定义并公开那个唯一的、组合好的顶层结构
pub struct Db {
    pub(crate) store: Storage,
    pub(crate) manager: EvictionManager,
}
impl Db {
    pub fn new() -> Self {
        Self {
            store: Storage::new(),
            manager: EvictionManager::new(),
        }
    }
}
