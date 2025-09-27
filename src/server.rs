use crate::core_aof::{AofMessage, aof_writer_task, explain_execute_aofcommand};
use crate::core_execute::{execute_command_normal};
use crate::core_explain::parse_frame;
use crate::core_time::start_time_caching_task;
use crate::db::Db;
use crate::error::Command::Unimplement;
use crate::error::{Command, Frame, KvError};
use bytes::{Buf, BytesMut};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender};


// 处理单个客户端连接的函数
pub async fn handle_connection(
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
/// 在缓冲区中查找 CRLF (`\r\n`) 的地道写法。
/// 如果找到，返回 `\r` 的位置索引。
fn find_crlf_idiomatic(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\r\n")
}
