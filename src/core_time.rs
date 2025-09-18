use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio;

// 1. 定义一个全局、原子性的 u64，用于存储缓存的毫秒级时间戳
pub static CACHED_TIME_MS: AtomicU64 = AtomicU64::new(0);

// 一个辅助函数，方便获取当前系统时间的毫秒戳
fn system_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// 在您的服务器启动时，只执行一次
pub fn start_time_caching_task() {
    // 初始化第一次的时间
    CACHED_TIME_MS.store(system_time_ms(), Ordering::Relaxed);

    // 启动一个独立的后台任务
    tokio::spawn(async move {
        loop {
            // 每 10 毫秒更新一次全局时间
            CACHED_TIME_MS.store(system_time_ms(), Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
}

// --- 在您处理命令的逻辑中 ---

// 任何需要时间戳的地方，不再调用 system_time_ms()，而是调用这个函数
pub fn get_cached_time_ms() -> u64 {
    CACHED_TIME_MS.load(Ordering::Relaxed)
}