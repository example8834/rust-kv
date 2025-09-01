use tokio::sync::mpsc::Receiver;
use tokio::time::{self, Duration};

pub async fn aof_writer_task(mut rx: Receiver<AofMessage>, path: &str) {
    // 打开 AOF 文件
    let mut file = BufWriter::new(OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .unwrap());

    // 创建一个每秒触发一次的定时器
    let mut interval = time::interval(Duration::from_secs(1));
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