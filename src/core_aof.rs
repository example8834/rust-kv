use std::error::Error;
use std::fs::File;

use std::io::Read;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc::Receiver;
use tokio::time::{self, Duration};

use crate::core_execute::{Db, execute_command};
use crate::core_explain::parse_frame;
use crate::error::{Command, KvError};
// 定义管道里传递的消息类型，这里就是序列化后的命令
pub type AofMessage = Vec<u8>;

pub async fn aof_writer_task(mut rx: Receiver<AofMessage>, path: &str) {
    // 打开 AOF 文件
    let mut file = BufWriter::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .unwrap(),
    );

    // 创建一个每秒触发一次的定时器
    let mut interval: time::Interval = time::interval(Duration::from_secs(1));
    let mut buffer: Vec<AofMessage> = Vec::with_capacity(128);

    loop {
        // 等待下一个定时器事件
        interval.tick().await;

        // 清空上次的缓冲区
        buffer.clear();

        // **核心的批量获取逻辑**
        // 尽最大努力，一次性从管道中取出所有等待的消息
        while let Ok(msg) = rx.try_recv() {
            buffer.push(msg);
        }

        if buffer.is_empty() {
            continue; // 这一秒没任务，直接开始下一次等待
        }

        // 批量写入文件
        for msg in &buffer {
            if let Err(e) = file.write_all(msg).await {
                tracing::error!("AOF 写入失败: {}", e);
            }
        }

        // 强制将系统缓冲区的数据刷到磁盘
        if let Err(e) = file.flush().await {
            tracing::error!("AOF 刷盘失败: {}", e);
        }
    }
}

pub async fn explain_execute_aofcommand(
    path: &str,
    db: &Db,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut file = File::open(path)?;
    //单线程恢复可以很大
    let mut file_data: Vec<u8> = vec![0; 1024 * 1024 * 512];
    //这个是应该写的区域 会随着不断变化
    let mut file_data_ref = &mut file_data[..];
    let mut exec_time;
    let mut tail_file_length = 0;
    loop {
        let size: usize = file.read(file_data_ref)? + tail_file_length;
        let mut tail_size: usize = 0;
        let mut data: &[u8] = &file_data[0..size];
        
        exec_time = 0;
        if size == 0 {
            break;
        }

        loop {
            if data.len() == 0 {
                return Ok(())
            }
            match parse_frame(data) {
                Ok(frame) => match frame {
                    //这个分支只有不可变
                    Some((frame, size)) => {
                        match Command::try_from(frame) {
                            Ok(frame) => {
                                execute_command(frame, db).await?;
                                data = &data[size..];
                                exec_time += 1;
                            }
                            Err(e) => {
                                return Err(e.into());
                            }
                        };
                        tail_size = size;
                    }
                    //这个分支有可变的复制 但是并没有使用可变 但是直接使用了对象  对象 不可变 可变三者交叉使用
                    //只要每个域都没有问题 就可以交叉使用 没有问题
                    None => {
                        file_data.copy_within(tail_size..size, 0);
                        // 5. 将剩下的数据移动到缓冲区头部
                        file_data_ref = &mut file_data[(size - tail_size)..];
                        tail_file_length = size - tail_size;
                        if exec_time == 0 {
                            return Err("字符串文件过大 大于512M".into());
                        }
                        break;
                    }
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}
