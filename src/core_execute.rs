use crate::aof_exchange::AofContent;
use crate::command_execute::{CommandContext, CommandExecutor};
use crate::context::{ConnectionContent, ConnectionState};
use crate::core_aof::AofMessage;
use crate::db::LockedDb;
use crate::error::{Command, EvalCommand, Expiration, Frame, IsAof, KvError, ToBulk};
use crate::{Db, db};
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
    let result = execute_command_hook(&command, Some(db.clone()), None, None).await;
    result
}

pub async fn execute_command_hook<'a>(
    command: &Command,
    db: Option<Db>, // post_write_hook 是一个可选的闭包
    connect_content: Option<ConnectionContent>,
    db_lock: Option<&mut LockedDb>,
) -> Result<Frame, KvError> {
    match command {
        Command::Get(get) => {
            get.execute(
                CommandContext {
                    db: None,
                    connect_content,
                },
                db_lock,
            )
            .await
        }
        Command::Set(set) => {
            set.execute(
                CommandContext {
                    db: None,
                    connect_content,
                },
                db_lock,
            )
            .await
        }
        Command::Ping(ping) => {
            ping.execute(
                CommandContext {
                    db: None,
                    connect_content,
                },
                None,
            )
            .await
        }
        Command::Unimplement(unimplement) => {
            unimplement
                .execute(
                    CommandContext {
                        db: None,
                        connect_content,
                    },
                    None,
                )
                .await
        }
        Command::EvalCommand(eval_command) => {
            eval_command
                .execute(
                    CommandContext {
                        db: db,
                        connect_content,
                    },
                    None,
                )
                .await
        }
    }
}

// AI 提供的正确代码，我帮你整理并解释
pub async fn execute_command_normal(
    command: Command,
    db: &Db,
    connect_content: ConnectionContent,
) -> Result<Frame, KvError> {
    //这里已经是脱离所有权了 开始独立拿出来用了
    let mut lock = get_command_lock(&command, db).await;
    let frame: Frame = execute_command_hook(
        &command,
        Some(db.clone()),
        Some(connect_content.clone()),
        lock.as_mut(),
    )
    .await?;
    //在这里同意执行aof 正常情况下的限定执行
    command.exe_aof_command(AofContent {
        aof_tx: &connect_content.aof_tx,
        shutdown_tx: &connect_content.shutdown_tx,
    }).await;
    Ok(frame)
}

pub async fn get_command_lock<'a>(command: &Command, db: &'a Db) -> Option<LockedDb> {
    match command {
        Command::Set(set_command) => db.store.lock_write(&set_command.key).await.into(),
        Command::Get(get_command) => db.store.lock_read(&get_command.key).await.into(),
        Command::Ping(_) => None,
        Command::Unimplement(_) => None,
        Command::EvalCommand(_) => None,
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
