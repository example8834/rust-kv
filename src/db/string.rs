use std::sync::Arc;

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
        manager: &mut EvictionManager,
    ) {
        manager.execute_write_op(key.clone(), &value);
        if let LockType::Write(ref mut map) = self.guard {
            map.insert(key, value);
        } else {
            panic!("Attempted to write with a read lock");
        };
    }

    pub fn get_string(self, key: Arc<String>, manager: &mut EvictionManager) -> Option<ValueEntry> {
        manager.execute_read_op(key.clone());
        if let LockType::Read(ref map) = self.guard {
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
        } else {
            panic!("Attempted to write with a read lock");
        };
    }
}
