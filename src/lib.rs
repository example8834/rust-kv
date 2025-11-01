mod aof_exchange;
mod command_exchange;
mod command_execute;
mod config;
mod context;
mod core_aof;
mod core_exchange;
mod core_execute;
mod core_explain;
mod core_time;
mod db;
mod error;
mod server;
mod shutdown;
mod types;

use crate::context::{CONN_STATE, ConnectionContent, ConnectionState};
use crate::core_aof::{AofMessage, aof_writer_task, explain_execute_aofcommand};
use crate::core_execute::execute_command_normal;
use crate::core_explain::parse_frame;
use crate::core_time::start_time_caching_task;
use crate::db::Db;
use crate::error::Command::Unimplement;
use crate::error::{Command, Frame, KvError};
use crate::server::handle_connection;
use bytes::{Buf, BytesMut};
use std::error::Error;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, Sender};
use tracing::span;

pub async fn run() {
    // 创建一个容量为 1024 的管道
    let (aof_tx, rx) = mpsc::channel::<AofMessage>(1024);
    //获取类型 这个广播
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    let aop_file_path = "database.aof";
    // 启动专门的 AOF 写入后台任务
    tokio::spawn(aof_writer_task(rx, aop_file_path));

    tracing_subscriber::fmt::init();
    // 1. 绑定监听地址
    // "127.0.0.1:6379" 是 Redis 的默认端口，我们沿用它可以方便地用 `redis-cli` 测试
    // 如果端口占用失败 直接报错退出
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();
    println!("服务器启动，监听于 127.0.0.1:6379");

    //创建db
    let mut db = Db::new();
    // 模拟一个新的客户端连接进来
    let client_addr = "192.168.1.10:54321".to_string();
    let initial_state = ConnectionState {
        selected_db: 0, // 默认连接到 1 号数据库
        client_address: Some(client_addr),
    };
    CONN_STATE
        .scope(initial_state, async {
            match explain_execute_aofcommand(aop_file_path, &mut db).await {
                Err(e) => {
                    panic!("aof 清理失败  {}", e)
                }
                _ => {
                    print!("aof数据恢复成功")
                }
            }
        })
        .await;
    //开始时间获取任务
    start_time_caching_task();
    /*
     * db克隆代价很小
     * 同时开启两个异步任务
     * 1.过期时间检测淘汰
     * 2.内存监听淘汰
     * 都是定时任务执行到主线程结束
     */
    db.clone().store.eviction_ttl();
    db.clone().store.eviction_memory(1024 * 1024 * 8);
    // 2. 接受连接循环
    loop {
        let connect_content = ConnectionContent {
            aof_tx: aof_tx.clone(),
            shutdown_tx: shutdown_tx.clone(),
        };
        let mut receiver = connect_content.shutdown_tx.subscribe();
        // 等待一个新的客户端连接
        // 并不是包裹了一层 所以整体代码侵入行为降低
        // 现在整体等待被包裹成两个了
        let (socket, addr) = tokio::select! {
            res = listener.accept() =>{
                match res {
                    Ok(res) => {
                        res
                    },
                    Err(_) => {
                        break;
                    },
                }
            }
            _ = receiver.recv() =>{
                break;
            }
        };
        // let (socket, _) = listener.accept().await;
        tracing::info!("接收到新连接");
        let db = db.clone();

        let initial_state = ConnectionState {
            selected_db: 0, // 默认连接到 1 号数据库
            client_address: Some(addr.to_string()),
        };
        // CONN_STATE
        //     .scope(initial_state, async {
        //         // 3. 为每个连接生成一个新的异步任务
        //         tokio::task::spawn(async move {
        //             // 在这个新任务中处理连接
        //             if let Err(e) = handle_connection(socket, db, tx_clone).await {
        //                 tracing::error!("处理时出错: {}", e);
        //             }
        //         });
        //     })
        //     .await;
        // 2. 【正确！】spawn 一个新任务
        tokio::task::spawn(async move {
            // 3. 【正确！】在新任务【内部】设置 TaskLocal
            CONN_STATE
                .scope(initial_state, async move {
                    // 现在，这个 handle_connection 任务
                    // 以及它调用的所有函数 (比如 lock_read)
                    // 都可以安全地调用 CONN_STATE.with() 了！
                    if let Err(e) = handle_connection(socket, db, connect_content).await {
                        tracing::error!("处理时出错: {}", e);
                    }
                })
                .await; // .await 这个 scope
        });
    }
}
