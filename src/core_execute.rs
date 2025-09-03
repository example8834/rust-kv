use crate::core_aof::AofMessage;
use crate::error::{Command, Frame, KvError};
use bytes::Bytes;
use std::collections::HashMap;
use std::f32::consts::E;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

// --- 1. 存储层：定义我们的内存数据库 ---
// Arc (Atomically Referenced Counter) 允许多个线程安全地共享所有权。
// Mutex (Mutual Exclusion) 保证在同一时间只有一个线程能修改 HashMap。
pub type Db = Arc<Mutex<HashMap<String, Bytes>>>;


// 正确的类型别名定义
type HookFn = for<'a> fn(&'a Command) -> Pin<Box<dyn Future<Output = Result<(), KvError>> + Send >>;

// 假定：Command: Clone
pub async fn execute_command(command: Command, db: &Db) -> Result<Frame, KvError> {
    // 在调用时直接转换 None 的类型
      // 这个调用现在是完全正确的，因为 `HookFn` 的定义和 `execute_command_hook` 的要求完美匹配
    let result = execute_command_hook(command, db, None as Option<HookFn>).await;
    result
}

pub async fn execute_command_hook<F>(
    command: Command,
    db: &Db, // post_write_hook 是一个可选的闭包
    // F 是闭包的类型
    // Fut 是闭包返回的 Future 类型
    mut post_write_hook: Option<F>,
) -> Result<Frame, KvError>
where
   // 将所有约束条件都明确地写在 F 上
    F: for<'a> FnMut(&'a Command) -> Pin<Box<dyn Future<Output = Result<(), KvError>> + Send >>,
{
    // 我们只对写命令应用钩子，先克隆一份给钩子用
    let cmd_for_hook = if matches!(&command, Command::Set { .. }) {
        Some(command.clone())
    } else {
        None
    };
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
            db_lock.insert(key, value);
            // 在锁被释放之前，执行钩子
            if let (Some(hook), Some(cmd)) = (post_write_hook.as_mut(), cmd_for_hook) {
                hook(&cmd).await?; // 等待钩子执行完成
            }
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

// AI 提供的正确代码，我帮你整理并解释
pub async fn execute_command_normal(
    command: Command,
    db: &Db,
    tx: Sender<AofMessage>, // 假设你已经改成了接收所有权的 Sender
) -> Result<Frame, KvError> {
    let tx = tx.clone();

    // 这个闭包是解决问题的关键
    let aof_callback = move |cmd: &Command|-> Pin<Box<dyn Future<Output = Result<(), KvError>> + Send>> { // 接收一个引用 &Command
        
        // 关键一步：立即克隆，得到一个拥有所有权的 Command
        // 这就“剪断”了与外部 &cmd 引用的生命周期联系
        let owned_cmd = cmd.clone();
        
        let tx = tx.clone();

        // 这个 async 块现在捕获的是 owned_cmd，它不再借用任何外部的东西
        // 因此，它创建的 Future 是自包含的（'static）
        Box::pin(async move {
            // 在内部使用 owned_cmd，而不是 cmd
            match owned_cmd {
                Command::Set { key, value } => {
                    let frame_aof = Frame::Array(vec![
                        Frame::Bulk(Bytes::from("SET")),
                        // 这里 key 和 value 是 move 进来的，不再需要 clone
                        Frame::Bulk(Bytes::from(key)),
                        Frame::Bulk(value),
                    ]);
                    if let Err(e) = tx.send(frame_aof.serialize()).await {
                        return Err(KvError::ProtocolError(format!("AOF send error: {}", e)));
                    }
                }
                _ => {}
            }
            Ok(())
        })
    };

    let frame: Frame = execute_command_hook(command, db, Some(aof_callback)).await?;
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
