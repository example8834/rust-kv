mod core_aof;
mod core_exchange;
mod core_execute;
mod core_explain;
mod error;
pub mod db;
pub mod command_execute;
pub mod command_exchange;
pub mod core_time;
pub mod aof_exchange;
pub mod server;

use crate::core_aof::{AofMessage, aof_writer_task, explain_execute_aofcommand};
use crate::core_execute::{execute_command_normal};
use crate::core_explain::parse_frame;
use crate::core_time::start_time_caching_task;
use crate::error::Command::Unimplement;
use crate::error::{Command, Frame, KvError};
use crate::server::handle_connection;
use bytes::{Buf, BytesMut};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender};
use db::Db;


pub async fn run() -> Result<(), Box<dyn Error>> {
    // 创建一个容量为 1024 的管道
    let (tx, rx) = mpsc::channel::<AofMessage>(1024);

    let aop_file_path = "database.aof";
    // 启动专门的 AOF 写入后台任务
    tokio::spawn(aof_writer_task(rx, aop_file_path));

    tracing_subscriber::fmt::init();
    // 1. 绑定监听地址
    // "127.0.0.1:6379" 是 Redis 的默认端口，我们沿用它可以方便地用 `redis-cli` 测试
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    println!("服务器启动，监听于 127.0.0.1:6379");

    //创建db
    let db = Db::default();

    match explain_execute_aofcommand(aop_file_path, &db).await {
        Err(e) => {
            panic!("aof 清理失败  {}", e)
        }
        _ => {
            print!("aof数据恢复成功")
        }
    }
    //开始时间获取任务
    start_time_caching_task();
    // 2. 接受连接循环
    loop {
        // 等待一个新的客户端连接
        let (socket, _) = listener.accept().await?;
        tracing::info!("接收到新连接");
        let db = db.clone();
        let tx_clone = tx.clone();
        // 3. 为每个连接生成一个新的异步任务
        tokio::spawn(async move {
            // 在这个新任务中处理连接
            if let Err(e) = handle_connection(socket, db, tx_clone).await {
                tracing::error!("处理时出错: {}", e);
            }
        });
    }
}