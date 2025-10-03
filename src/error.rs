use bytes::Bytes;
use std::{io, sync::Arc};
use thiserror::Error;


// 1. 定义我们自己的错误类型
#[derive(Debug, Error)]
pub enum KvError {
    #[error("IO 错误: {0}")]
    Io(#[from] io::Error),

    #[error("协议解析错误: {0}")]
    ProtocolError(String),

    #[error("意外的连接关闭")]
    UnexpectedEof,

    #[error("暂时没有实现")]
    Unimplement,

    #[error("无意义错误")]
    None,
}

// 2. 定义客户端可以发送的命令
#[derive(Debug,Clone)]
pub enum Command {
    Set(SetCommand), // 不再有 { ... }，而是直接包裹 Set 结构体
    Get(GetCommand),
    Ping(PingCommand),
    Unimplement(UnimplementCommand)
}


// 每一个 struct 现在都是一个独立的、清晰的命令“实体”
#[derive(Debug, Clone)]
pub struct SetCommand {
    pub key: Arc<String>,
    pub value: Bytes,
    pub expiration: Option<Expiration>, 
    pub condition: Option<SetCondition>
}

#[derive(Debug, Clone)]
pub struct GetCommand {
    pub key: Arc<String>,
}

#[derive(Debug, Clone)]
pub struct PingCommand {
    pub value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnimplementCommand {
    pub command: String,
    pub args: Vec<Bytes>,
}

#[derive(Debug,Clone)]
pub enum Expiration {
    EX(u64), // 秒
    PX(u64), // 毫秒
    EXAT(u64), // 秒
    PXAT(u64), // 毫秒
}

#[derive(Debug,Clone)]
pub enum SetCondition {
    NX, // Not Exists
    XX, // Exists
}
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Simple(String),
    Bulk(Bytes),
    Array(Vec<Frame>),
    Integer(i64),
    Null,
    Error(String),
}

pub enum ToBulk {
    String(String),
    Btyes(Bytes),
    Integer(i64)
}

pub enum IsAof {
    Yes,
    No
}