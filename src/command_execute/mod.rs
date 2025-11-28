
use bytes::Bytes;
use itoa::Buffer;

use crate::{
    context::ConnectionContent, core_time::get_cached_time_ms, db::{Db, LockedDb}, error::{Frame, KvError}
};
 mod common;
 mod string;
 #[derive(Clone)]
pub struct CommandContext {
    pub db: Option<Db>,
    pub connect_content: Option<ConnectionContent>
    
}

pub trait CommandExecutor {
      // 模板方法
    fn execute(
        &self,
        // ✅ 核心改动：从 &mut CommandContext 变成了 &CommandContext
        ctx:  CommandContext,
        db_lock: Option<& mut LockedDb>
    ) -> impl std::future::Future<Output = Result<Frame, KvError>> + Send ;

    // “原语”方法
    // async fn execute_data_resolve<'ctx>(
    //     &self,
    //     // ✅ 同步修改
    //     ctx: &'ctx CommandContext<'ctx>,
    // ) -> Result<Frame, KvError>;

    // // “钩子”方法
    // async fn execute_db<'ctx>(
    //     &self,
    //     // ✅ 同步修改
    //     ctx: &'ctx CommandContext<'ctx>,
    // ) -> Result<Frame, KvError> {
    //     Ok(Frame::Simple("OK".to_string()))
    // }
}
// 修正后的方法，返回一个可以存储的u64相对时间戳
pub fn calculate_expiration_timestamp_ms(expiration: &crate::error::Expiration) -> u64 {
    let now = get_cached_time_ms();
    match expiration {
        crate::error::Expiration::PX(ms) => now + ms,
        crate::error::Expiration::EX(s) => now + s * 1000,
        crate::error::Expiration::EXAT(s) => *s,
        crate::error::Expiration::PXAT(ms) => *ms,
    }
}
//高效的int 转byte 方法
pub fn parse_int_from_bytes(i: i64) -> Bytes {
    let mut buffer = Buffer::new();

    // 2. 将数字格式化到缓冲区中，返回一个指向缓冲区内容的 &str
    let printed_str = buffer.format(i);

    // 3. 从结果切片创建 Bytes (这里有一次复制，但避免了堆分配)
    Bytes::copy_from_slice(printed_str.as_bytes())
}

// 一个直接从 Bytes 高效解析 i64 的函数
pub fn bytes_to_i64_fast(b: &Bytes) -> Option<i64> {
    // 顯式標註 result 變量的類型
    // 直接告訴 parse 函數，你想解析成 i64
    let result = lexical_core::parse::<i64>(b);
    result.ok()
}
