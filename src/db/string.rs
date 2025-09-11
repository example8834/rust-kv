use bytes::Bytes;

use crate::db::{Element, LockedDb, ValueEntry, bytes_to_i64_fast};

impl<'a> LockedDb<'a> {
    pub fn set_string(&mut self, key: String, value: Bytes, time_expire: Option<u64>) {
        let value_obj;
        match bytes_to_i64_fast(&value) {
            Some(i) => {
                value_obj = ValueEntry {
                    data: crate::db::Value::Simple(Element::Int(i)),
                    expires_at: time_expire,
                }
            }
            None => {
                value_obj = ValueEntry {
                    data: crate::db::Value::Simple(Element::String(value)),
                    expires_at: time_expire,
                }
            }
        };
        self.guard.insert(key, value_obj);
    }
}
