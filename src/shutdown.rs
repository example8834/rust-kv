use tokio::signal;
use tokio::sync::broadcast; // 用广播来通知所有任务

pub struct service{
    
}
// (这是在 Unix/Linux/macOS 上的写法)
#[cfg(unix)]
async fn shutdown_listener(shutdown_tx: broadcast::Sender<()>) {
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