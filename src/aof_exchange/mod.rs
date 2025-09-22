use std::f32::consts::E;

use bytes::Bytes;
use itoa::Buffer;

use crate::{
    command_execute::CommandContext,
    core_time::get_cached_time_ms,
    error::{Frame, KvError},
};

pub mod string;

pub trait CommandAofExchange {
    // execute 方法現在接收 CommandContext 作為參數！
     fn execute_aof<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: &'ctx CommandContext<'ctx>,
    ) -> impl std::future::Future<Output = Result<Frame, KvError>> + Send;
}

pub fn exchange_absolute_time(expire_time: u64) -> Bytes {
    parse_int_from_bytes(get_cached_time_ms() + expire_time)
}
//高效的int 转byte 方法
pub fn parse_int_from_bytes(i: u64) -> Bytes {
    let mut buffer = Buffer::new();

    // 2. 将数字格式化到缓冲区中，返回一个指向缓冲区内容的 &str
    let printed_str = buffer.format(i);

    // 3. 从结果切片创建 Bytes (这里有一次复制，但避免了堆分配)
    Bytes::copy_from_slice(printed_str.as_bytes())
}