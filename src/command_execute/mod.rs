use std::{sync::OnceLock, time::{Duration, Instant}};

use tokio::sync::mpsc::Sender;

use crate::{core_aof::AofMessage, db::Db, error::{Frame, KvError}};
pub mod string;

pub struct CommandContext<'a> {
    pub db: &'a Db,
    pub tx: &'a Option<Sender<AofMessage>>
}


pub trait CommandExecutor {
    // execute 方法現在接收 CommandContext 作為參數！
     async fn execute<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: &'ctx mut CommandContext<'ctx>
    ) -> Result<Frame, KvError>;
}
// 使用 OnceLock 来延迟初始化 START_TIME
static START_TIME: OnceLock<Instant> = OnceLock::new();

/// 获取从程序启动到现在的毫秒数（单调递增）
fn monotonic_time_ms() -> u64 {
    // 第一次调用 .get_or_init() 时，会执行 Instant::now() 并存储结果
    // 之后的调用会直接返回已存储的值
    let start_time = START_TIME.get_or_init(|| Instant::now());
    start_time.elapsed().as_millis() as u64
}

// 修正后的方法，返回一个可以存储的u64相对时间戳
pub fn calculate_expiration_timestamp_ms(expiration: &crate::error::Expiration) -> u64 {
    let now = monotonic_time_ms();
    match expiration {
        crate::error::Expiration::PX(ms) => now + ms,
        crate::error::Expiration::EX(s) => now + s * 1000,
    }
}