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
    let mut command_context = CommandContext { db, tx: &tx };
    match command {
        Command::Get(get) => get.execute(&mut command_context).await,
        Command::Set(set) => set.execute(&mut command_context).await,
        Command::Ping(ping) => ping.execute(&mut command_context).await,
        Command::Unimplement(unimplement) => unimplement.execute(&mut command_context).await,
    }
}

pub fn str_to_bluk(bulk: ToBulk) -> Frame {
    match bulk {
        ToBulk::String(e) => Frame::Bulk(Bytes::from(e)),
        ToBulk::Btyes(bytes) => Frame::Bulk(bytes),
        ToBulk::Integer(integer) => Frame::Integer(integer),
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

/// 一个直接从 Bytes 高效解析 i64 的函数
fn bytes_to_i64_fast(b: &Bytes) -> Option<i64> {
    // 顯式標註 result 變量的類型
    // 直接告訴 parse 函數，你想解析成 i64
    let result = lexical_core::parse::<i64>(b);
    result.ok()
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
/// 获取当前时间的毫秒级 UNIX 时间戳 (u64)
fn current_timestamp_ms() -> u64 {
    // 1. 获取当前的 SystemTime
    let now = SystemTime::now();

    // 2. 计算从 UNIX 纪元到现在的持续时间 (Duration)
    // .duration_since() 会返回一个 Result，因为如果系统时间被设置到了 1970 年以前，
    // 这个操作会失败。对于服务器来说，这种情况属于灾难性的系统配置错误，
    // 直接 unwrap() 让程序 panic 是一个合理的选择。
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("System time is before the UNIX epoch, please check your system clock!");

    // 3. 将 Duration 转换为毫秒。as_millis() 返回一个 u128，
    // 这是为了防止未来几千年后的时间戳溢出 u64。
    let millis_u128 = duration_since_epoch.as_millis();

    // 4. 在当前和可预见的未来，这个值可以安全地转换为 u64。
    millis_u128 as u64
}
