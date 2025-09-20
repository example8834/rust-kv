use bytes::Bytes;

use crate::{
    core_time::get_cached_time_ms,
    db::{Element, LockType, LockedDb, ValueEntry, bytes_to_i64_fast, parse_int_from_bytes},
};

impl<'a> LockedDb<'a> {
    pub fn set_string(&mut self, key: String, value: ValueEntry) {
        if let LockType::Write(ref mut guard) = self.guard {
            guard.insert(key, value);
            return;
        }
    }

    pub fn get_string(&self, key: &str) -> Option<ValueEntry> {
        if let LockType::Read(guard) = &self.guard {
            if let Some(entry) = guard.get(key) {
                let time_expires = entry.expires_at;
                if let Some(expire_time) = time_expires {
                    if get_cached_time_ms() > expire_time {
                        return None;
                    }
                }
                return Some(entry.clone())
                // match &entry.data {
                //     crate::db::Value::Simple(Element::String(bytes)) => Some(bytes.clone()),
                //     crate::db::Value::Simple(Element::Int(i)) => Some(parse_int_from_bytes(*i)),
                //     _ => None,
                // }
            }
        } 
        return None;
    }
}
