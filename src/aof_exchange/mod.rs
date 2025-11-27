
use bytes::Bytes;

use itoa::Buffer;
use tokio::sync::mpsc::Sender;

use crate::{
    core_time::get_cached_time_ms,
    error::Command,
};

mod string;

pub trait CommandAofExchange {
    // execute 方法現在接收 CommandContext 作為參數！
    async fn execute_aof<'a>(
        &self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: AofContent<'a>,
    );
}

/*
 基于这个command 指令 实现对应方法 
 模块是分开的 并不一定就是代表数据结构是分开的 都是针对command 这个命令的
 所以一个模块是功能性划分 结构是实体划分 承载结构
 */
impl Command {
    pub async fn exe_aof_command<'a>(&self,ctx: AofContent<'a>){
        match self {
            Command::Set(set_command) => set_command.execute_aof(ctx).await,
            Command::Get(get_command) => todo!(),
            Command::Ping(ping_command) => todo!(),
            Command::Unimplement(unimplement_command) => todo!(),
            Command::EvalCommand(eval_command) => todo!(),
        }
    }
}


#[derive(Clone, Debug)]
pub struct AofContent<'a> {
    pub aof_tx: &'a Sender<Vec<u8>>,
    pub shutdown_tx: &'a tokio::sync::broadcast::Sender<()>
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