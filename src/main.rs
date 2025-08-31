mod core_exchange;
mod core_execute;
mod core_explain;
mod error;

use crate::core_execute::{Db, execute_command};
use crate::core_explain::parse_frame;
use crate::error::{Command, Frame, KvError};
use bytes::{Buf, BytesMut};
use std::error::Error;
use std::io::BufRead;
use std::ops::Index;
use std::ptr::read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use crate::error::Command::Unimplement;

#[tokio::main] // 这个宏将 main 函数标记为 Tokio 运行时入口点
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    // 1. 绑定监听地址
    // "127.0.0.1:6379" 是 Redis 的默认端口，我们沿用它可以方便地用 `redis-cli` 测试
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    println!("服务器启动，监听于 127.0.0.1:6379");

    let db = Db::default();
    // 2. 接受连接循环
    loop {
        // 等待一个新的客户端连接
        let (socket, _) = listener.accept().await?;
        tracing::info!("接收到新连接");
        let db = db.clone();
        // 3. 为每个连接生成一个新的异步任务
        tokio::spawn(async move {
            // 在这个新任务中处理连接
            if let Err(e) = handle_connection(socket, db).await {
                tracing::error!("处理时出错: {}", e);
            }
        });
    }
}

/// 在缓冲区中查找 CRLF (`\r\n`) 的地道写法。
/// 如果找到，返回 `\r` 的位置索引。
fn find_crlf_idiomatic(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\r\n")
}

// 处理单个客户端连接的函数
async fn handle_connection(mut socket: TcpStream, db: Db) -> Result<(), Box<dyn Error + Send + Sync>> {
    // 1. 使用 Vec<u8> 作为缓冲区
    let mut buf = BytesMut::with_capacity(1024);
    // 目前来说用的模式是1 是 redis 格式 0 是单个字符模式
    let type_fix = 1;
    // 4. 在该连接的循环中读取数据
    loop {
        let n = socket.read_buf(&mut buf).await?;

        // 如果 read 返回 0，表示客户端关闭了连接
        if n == 0 {
            println!("客户端关闭连接");
            return Ok(());
        }

        if type_fix == 0 {
            if let Some(index) = find_crlf_idiomatic(&buf) {
                println!("{}", index);
                // try_parse_command_RESP(&buf[..index], &mut socket).expect("命令处理错误");

                println!("接收到 {} 字节:  {:?}", n, &buf[..n]);

                // 5. 【Echo 逻辑】将收到的数据原封不动写回给客户端
                socket.write_all(&buf[..index + 2]).await?;

                buf.advance(index + 2);
            }
        } else {
            println!("{:?}",std::str::from_utf8(&buf));
            match explain_execute_command(&mut buf, &db).await {
                Ok(result) => {
                    for item in result {
                        socket.write_all(&item).await?;
                    }
                }
                Err(e) => {  // 转换失败（语义错误），准备一个错误响应
                    let error_response = Frame::Error(e.to_string());
                    socket.write_all(&error_response.serialize()).await?;
                    // 继续处理缓冲区里的下一个命令
                    continue;}
            };
        }

        println!("已回送数据");
    }
}

async fn explain_execute_command(buf: &mut BytesMut, db: &Db) -> Result<Vec<Vec<u8>>, Box<dyn Error + Send + Sync>> {
    let mut vec_result: Vec<Vec<u8>> = Vec::new();
    while let Ok(Some(frame)) = parse_frame(buf) {
        match Command::try_from(frame) {
            Ok(frame) => {
                match frame {
                    _=> {
                        let result = execute_command(frame, db).await?;
                        vec_result.push(result.serialize());
                    }
                }
            }
            Err(e) => return Err(e.into())
        }
    }
    Ok(vec_result)
}
