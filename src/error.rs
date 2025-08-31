use bytes::Bytes;
use std::io;
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
#[derive(Debug)]
pub enum Command {
    Set {
        key: String,
        value: Bytes, // 值可以是任意字节，所以用 Vec<u8>
    },
    Get {
        key: String,
    },
    Unimplement {
        command: String,
        args: Vec<Bytes>,
    },

    PING {
        value: Option<String>,
    }, // 我们可以稍后再添加 Del, Ping 等其他命令
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
