use tokio::{sync::mpsc::Sender, task_local};

// 定义我们想为每个任务独立存储的状态
#[derive(Clone, Debug)]
pub struct ConnectionState {
    pub selected_db: usize,
    pub client_address: Option<String>,
}
// 定义我们想为每个任务独立存储的状态
#[derive(Clone, Debug)]
pub struct ConnectionContent {
    pub aof_tx: Sender<Vec<u8>>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

// 使用 task_local! 宏来声明一个名为 CONN_STATE 的“插槽”
// pub static 意味着其他模块也可以访问这个“插槽”
task_local! {
    pub static CONN_STATE: ConnectionState;
}