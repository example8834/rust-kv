use crate::error::{Command, Frame, KvError};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// --- 1. 存储层：定义我们的内存数据库 ---
// Arc (Atomically Referenced Counter) 允许多个线程安全地共享所有权。
// Mutex (Mutual Exclusion) 保证在同一时间只有一个线程能修改 HashMap。
pub type Db = Arc<Mutex<HashMap<String, Bytes>>>;

// --- 2. 执行层：实现命令执行逻辑 ---
// 关键改动：因为 .lock() 现在是异步操作，所以整个函数也必须是 async fn
pub async fn execute_command(command: Command, db: &Db) -> Result<Frame, KvError> {
    match command {
        Command::Get { key } => {
            // 1. 异步地锁定数据库以获取访问权。
            //    .lock() 返回一个 Future，我们必须 .await 它。
            //    它在成功时直接返回锁，失败（中毒）时会 panic，但在 Tokio 中通常更健壮。
            let db_lock = db.lock().await;

            // 2. 使用 .get() 方法查找 key。
            if let Some(value) = db_lock.get(&key) {
                // 3. 如果找到了，返回一个包含值的 Frame::Bulk。
                Ok(Frame::Bulk(value.clone()))
            } else {
                // 4. 如果没找到，返回一个 Frame::Null。
                Ok(Frame::Null)
            }
        }
        Command::Set { key, value } => {
            // 1. 异步地锁定数据库以获取可变访问权。
            let mut db_lock = db.lock().await;

            // 2. 使用 .insert() 方法插入键值对。
            db_lock.insert(key, value);

            // 3. 返回一个表示成功的 Frame::Simple("OK")。
            Ok(Frame::Simple("OK".to_string()))
        }
        Command::PING { value } => {
            if let Some(msg) = value {
                Ok(Frame::Bulk(Bytes::from(msg)))
            } else {
                Ok(Frame::Simple("PONG".to_string()))
            }
        }
        Command::Unimplement{ command, .. }  =>   Ok(Frame::Error(format!("ERR unknown command '{}'", command))),
    }
}

impl Frame {
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Frame::Simple(s) => format!("+{}\r\n", s).into_bytes(),
            Frame::Error(s) => format!("-{}\r\n", s).into_bytes(),
            Frame::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            Frame::Null => b"$-1\r\n".to_vec(),
            Frame::Bulk(bytes) => {
                let mut buf = format!("${}\r\n", bytes.len()).into_bytes();
                buf.extend_from_slice(bytes);
                buf.extend_from_slice(b"\r\n");
                buf
            }
            Frame::Array(frames) => {
                let mut buf = format!("*{}\r\n", frames.len()).into_bytes();
                for frame in frames {
                    buf.extend_from_slice(&frame.serialize());
                }
                buf
            }
        }
    }
}
