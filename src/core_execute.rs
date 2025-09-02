use crate::core_aof::AofMessage;
use crate::error::{Command, Frame, KvError};
use bytes::Bytes;
use std::collections::HashMap;
use std::f32::consts::E;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

// --- 1. 存储层：定义我们的内存数据库 ---
// Arc (Atomically Referenced Counter) 允许多个线程安全地共享所有权。
// Mutex (Mutual Exclusion) 保证在同一时间只有一个线程能修改 HashMap。
pub type Db = Arc<Mutex<HashMap<String, Bytes>>>;

// 假定：Command: Clone
pub async fn execute_command(command: Command, db: &Db) -> Result<Frame, KvError> {
    match command {
        Command::Get { key } => {
            let db_lock = db.lock().await;
            if let Some(value) = db_lock.get(&key) {
                Ok(Frame::Bulk(value.clone()))
            } else {
                Ok(Frame::Null)
            }
        }
        Command::Set { key, value } => {
            let mut db_lock = db.lock().await;
            db_lock.insert(key.clone(), value.clone());
            Ok(Frame::Simple("OK".to_string()))
        }
        Command::PING { value } => {
            if let Some(msg) = value {
                Ok(Frame::Bulk(Bytes::from(msg)))
            } else {
                Ok(Frame::Simple("PONG".to_string()))
            }
        }
        Command::Unimplement { command, .. } => {
            Ok(Frame::Error(format!("ERR unknown command '{}'", command)))
        }
    }
}

pub async fn execute_command_normal(
    command: Command,
    db: &Db,
    tx: &Sender<AofMessage>,
) -> Result<Frame, KvError> {
    // clone 一份命令用于可能的 AOF 写入
    let cmd_for_aof = command.clone();

    // 执行命令
    let frame: Frame = execute_command(command, db).await?;

    match cmd_for_aof {
        Command::Set { key, value } => {
            let frame_aof = Frame::Array(vec![
                Frame::Bulk(Bytes::from("SET")),
                Frame::Bulk(Bytes::from(key)),
                Frame::Bulk(value),
            ]);
            match  tx.send(frame_aof.serialize()).await {
                Ok(_) => todo!(),
                Err(_) => todo!(),
            };
        }
        _ => {

        }
    }
    Ok(frame)
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
