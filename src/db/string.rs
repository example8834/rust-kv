use std::sync::{Arc, atomic::Ordering};

use crate::{
    core_time::get_cached_time_ms,
    db::{LockType, LockedDb, eviction::EvictionManager},
    types::ValueEntry,
};
impl<'a> LockedDb<'a> {
    // --- 现在你的 set_string 方法变得极其清晰 ---
    pub fn set_string(
        mut self,
        key: Arc<String>,
        value: ValueEntry,
    ) {
        if let LockType::Write(ref mut map) = self.guard {
            //let mut db_store = map.db_store.clone();
            let size_before= match map.db_store.get(&key){
                Some(entry) => entry.data_size,
                None => 0,
            };
            //值差异
            let size_differ = value.data_size - size_before;
            //添加内存使用情况
            map.approx_memory.fetch_add(size_differ, Ordering::Relaxed);
            //插入数值的时候 消耗掉这个
            map.db_store.insert(key, value);
        } else {
            panic!("Attempted to write with a read lock");
        };
    }

    pub fn get_string(self, key: Arc<String>) -> Option<ValueEntry> {
        if let LockType::Read(ref map) = self.guard {
            if let Some(entry) = map.db_store.get(&key) {
                let time_expires = entry.expires_at;
                if let Some(expire_time) = time_expires {
                    if get_cached_time_ms() > expire_time {
                        return None;
                    }
                }
                return Some(entry.clone());
            }
            return None;
        } else {
            panic!("Attempted to write with a read lock");
        };
    }
}
