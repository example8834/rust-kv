use bytes::Bytes;

use crate::db::{bytes_to_i64_fast, monotonic_time_ms, parse_int_from_bytes, Element, LockedDb, ValueEntry};

impl<'a> LockedDb<'a> {
    pub fn set_string(&mut self, key: String, value: ValueEntry, time_expire: Option<u64>) {
        self.guard.insert(key, value);
    }

    pub fn get_string(&self, key: &str) -> Option<ValueEntry> {
        if let Some(entry) = self.guard.get(key) {
            let time_expires = entry.expires_at;
            if let Some(expire_time) = time_expires {
                if monotonic_time_ms() > expire_time {
                    return None;
                }
            }
            Some(entry.clone())
            // match &entry.data {
            //     crate::db::Value::Simple(Element::String(bytes)) => Some(bytes.clone()),
            //     crate::db::Value::Simple(Element::Int(i)) => Some(parse_int_from_bytes(*i)),
            //     _ => None,
            // }
        } else {
            None
        }
    }
}
