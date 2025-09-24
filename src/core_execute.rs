use crate::Db;
use crate::command_execute::{CommandContext, CommandExecutor};
use crate::core_aof::AofMessage;
use crate::db::{Element, Value, ValueEntry};
use crate::error::{Command, Expiration, Frame, IsAof, KvError, ToBulk};
use bytes::Bytes;
use itoa::Buffer;
use std::f32::consts::E;
use std::pin::{self, Pin};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tracing_subscriber::registry::Data;

// 假定：Command: Clone
pub async fn execute_command(command: Command, db: &Db) -> Result<Frame, KvError> {
    // 在调用时直接转换 None 的类型
    // 这个调用现在是完全正确的，因为 `HookFn` 的定义和 `execute_command_hook` 的要求完美匹配
    let result = execute_command_hook(command, db, None).await;
    result
}

pub async fn execute_command_hook(
    command: Command,
    db: &Db, // post_write_hook 是一个可选的闭包
    tx: Option<Sender<AofMessage>>,
) -> Result<Frame, KvError> {
    let  command_context = CommandContext { db, tx: &tx };
    match command {
        Command::Get(get) => get.execute(&command_context).await,
        Command::Set(set) => set.execute(&command_context).await,
        Command::Ping(ping) => ping.execute( &command_context).await,
        Command::Unimplement(unimplement) => unimplement.execute(& command_context).await,
    }
}

// AI 提供的正确代码，我帮你整理并解释
pub async fn execute_command_normal(
    command: Command,
    db: &Db,
    tx: Sender<AofMessage>, // 假设你已经改成了接收所有权的 Sender
) -> Result<Frame, KvError> {
    let frame: Frame = execute_command_hook(command, db, Some(tx)).await?;
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