use std::mem;
use std::sync::Arc;

use tokio::signal;
use tokio::sync::broadcast::Sender;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle; // 用广播来通知所有任务
use futures::future::join_all;

pub struct ShutDown{
    pub aof_task : JoinHandle<()>,
    pub time_task : JoinHandle<()>,
    pub eviction_memory_task: JoinHandle<Arc<Mutex<Vec<JoinHandle<()>>>>>,
    pub eviction_ttl_task :JoinHandle<()>,
    pub connect_task: JoinHandle<Arc<Mutex<Vec<JoinHandle<()>>>>>,
    pub infra_shutdown_tx: Sender<()>
}
impl ShutDown{
    pub async fn shutdown(self){
       print!("开始执行");
       //接收任务关闭
       let connect = self.connect_task.await.unwrap();
       //所有权这个 必须获取锁 然后内部置换 对象共享的 你不能直接消费所有权 所以只能置换出来所有权 
       let connect_handle_vec = mem::take(&mut *connect.lock().await); // 把 Vec 拿走，guard 里变成 Vec::new()
       //等待数组内部的连接
       join_all(connect_handle_vec).await;

       //淘汰算法的停止 
       let _ = self.eviction_ttl_task.await;
       let eviction_memory =  self.eviction_memory_task.await.unwrap();
       let eviction_memory_vec = mem::take(&mut *eviction_memory.lock().await);
       //等待数组内部的连接
       join_all(eviction_memory_vec).await;

       //aof缓存停止
       let _ = self.aof_task.await;

       //时间更新停止
       self.infra_shutdown_tx.send(()).expect("failed to send shutdown broadcast");
       let _ = self.time_task.await;
    }
}

// (这是在 Unix/Linux/macOS 上的写法)
#[cfg(unix)]
pub async fn shutdown_listener(shutdown_tx: broadcast::Sender<()>) {
    // 1. 监听 Ctrl+C (SIGINT)
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    };

    // 2. 监听 SIGTERM (生产环境的关闭信号)
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to listen for sigterm")
            .recv()
            .await;
    };

    // 3. 用 select! 同时等待这两者！
    tokio::select! {
        _ = ctrl_c => {
            println!("收到 SIGINT (Ctrl+C)... 准备关闭");
        },
        _ = terminate => {
            println!("收到 SIGTERM (生产环境关闭信号)... 准备关闭");
        },
    }

    // 4. 无论收到哪个，都发送 *同一个* “内部关闭”广播
    shutdown_tx.send(()).expect("failed to send shutdown broadcast");
}


// (Windows 有点不一样，它不区分 SIGINT 和 SIGTERM)
#[cfg(windows)]
async fn shutdown_listener(shutdown_tx: broadcast::Sender<()>) {
    // 在 Windows 上，ctrl_c() 会同时处理 SIGINT 和 SIGTERM
    signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    println!("收到关闭信号... 准备关闭");
    shutdown_tx.send(()).expect("failed to send shutdown broadcast");
}