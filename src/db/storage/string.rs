use std::sync::Arc;

use crate::{
    core_time::get_cached_time_ms,
    db::{storage::LockedDb, EvictionManager}, types::{ValueEntry},
};
impl<'a> LockedDb<'a> {
    // --- 现在你的 set_string 方法变得极其清晰 ---
    pub fn set_string(mut self, key: Arc<String>, value: ValueEntry, manager:&mut EvictionManager) {
        // 我们调用模板方法，并传入一个只关心“插入”这件小事的闭包
        self.execute_write_op(key, value,manager, |key, value,manage, map| {
            map.insert(key, value);
        });
    }

    pub fn get_string(self, key: Arc<String>,manager:&mut EvictionManager) -> Option<ValueEntry> {
        self.execute_read_op(key,manager, |key,manager, map| {
            if let Some(entry) = map.get(&key) {
                let time_expires = entry.expires_at;
                if let Some(expire_time) = time_expires {
                    if get_cached_time_ms() > expire_time {
                        return None;
                    }
                }
                return Some(entry.clone());
            }
            return None;
        })
    }
}
