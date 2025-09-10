mod core_aof;
mod core_exchange;
mod core_execute;
mod core_explain;
mod error;
pub mod db;
pub mod command_execute;
pub mod command_exchange;

use crate::core_aof::{AofMessage, aof_writer_task, explain_execute_aofcommand};
use crate::core_execute::{execute_command_normal};
use crate::core_explain::parse_frame;
use crate::error::Command::Unimplement;
use crate::error::{Command, Frame, KvError};
use bytes::{Buf, BytesMut};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender};
use db::Db;

#[tokio::main] // 这个宏将 main 函数标记为 Tokio 运行时入口点
async fn main() -> Result<(), Box<dyn Error>> {
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

/// 在缓冲区中查找 CRLF (`\r\n`) 的地道写法。
/// 如果找到，返回 `\r` 的位置索引。
fn find_crlf_idiomatic(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\r\n")
}

// 处理单个客户端连接的函数
async fn handle_connection(
    mut socket: TcpStream,
    db: Db,
    tx: Sender<AofMessage>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
            println!("{:?}", std::str::from_utf8(&buf));
            match explain_execute_command(&mut buf, &db, &tx).await {
                Ok(result) => {
                    print!("{}",result.len());
                    for item in result {
                        socket.write_all(&item).await?;
                    }
                }
                Err(e) => {
                    // 转换失败（语义错误），准备一个错误响应
                    let error_response = Frame::Error(e.to_string());
                    socket.write_all(&error_response.serialize()).await?;
                    //错误处理 裁减掉错误指令
                    match buf.windows(2).position(|window| window == b"*") {
                        Some(index) => {
                            buf.advance(index);
                        }
                        None => {
                            buf.clear();
                        }
                    }
                    // 继续处理缓冲区里的下一个命令
                    continue;
                }
            };
        }

        println!("已回送数据");
    }
}

async fn explain_execute_command(
    buf: &mut BytesMut,
    db: &Db,
    tx: &Sender<AofMessage>,
) -> Result<Vec<Vec<u8>>, Box<dyn Error + Send + Sync>> {
    let mut vec_result: Vec<Vec<u8>> = Vec::new();
    let mut vec: &[u8] = buf.as_ref();
    let mut total_size: usize = 0;
    /**
     * 首先盘点一下 由于分层 并且命令是字符串 所以每层都有可能出现错误
     * 1.第一层就是字符串解析成frame层 这个层面会出现的错误有 这个层面 只看是否能结构化成frame 和 具体指令要求无关
     *  1.解析过程中 首先就是发现命令没有传输完成就直接跳过
     *  2.发现比如格式错误
     *    1.中间/r/n没有
     *    2.字符串长度和实际标注不匹配
     *  第一层总体来说就是协议报错 是最底层的问题
     * 2.第二层就是frame 转换成command 这个就是要对于frame 生成结构严整
     *  1.首先就是遇到未知指令 返回直接返回说命令没有实现
     *  2.经典的命令长度不匹配 直接返回错误
     * 这一层是指令格式校验
     * 3.执行层面的话 这里错误比较少
     *   1.一半就是按照校验执行就行 执行出错的时候很少
     *   2.就是兼容没有实现的指令 这一步返回特定返回值 不需要再上一层就直接返回错误
     */
    while let Ok(Some((frame, size))) = parse_frame(vec) {
        match Command::try_from(frame) {
            //这个错误事第一个指令就错误的错误 就是结构性质错误
            Ok(frame) => match frame {
                _ => {
                    //这个事指令错误 而不是结构化错误
                    let result = execute_command_normal(frame, db, tx.clone()).await?;
                    vec_result.push(result.serialize());
                    vec = &vec[size..];
                    total_size += size;
                }
            },
            Err(e) => {
                buf.advance(total_size);
                return Err(e.into());
            }
        }
    }
    buf.advance(total_size);
    Ok(vec_result)
}
