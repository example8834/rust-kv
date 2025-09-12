use std::ops::Index;

use crate::command_exchange::CommandExchange;
use crate::error::Command::{Get, Ping, Set, Unimplement};
use crate::error::KvError::ProtocolError;
use crate::error::{Command, Expiration, Frame, GetCommand, KvError, SetCommand, SetCondition};
use bytes::Bytes;

impl TryFrom<Frame> for Command {
    type Error = KvError;

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        let  frames = match frame {
            Frame::Array(frames) => frames,
            _ => return Err(ProtocolError("must be a array from frame".into())),
        };
        if frames.is_empty() {
            return Err(ProtocolError("frame is empty".into()));
        }
        let length = frames.len();
        let mut iter: std::vec::IntoIter<Frame> = frames.into_iter();
        match iter.next() {
         Some(Frame::Bulk(start_str)) => {
                let command_name = String::from_utf8(start_str.to_vec())
                    .map_err(|_| ProtocolError("zhuan huan yi chang ".into()))?
                    .to_uppercase();
                match command_name.as_str() {
                    "GET" => {
                        if length != 2 {
                            return Err(ProtocolError("GET 命令需要 1 个参数".into()));
                        }
                        GetCommand::exchange(iter)
                    }
                    "SET" => {
                        if length < 3 {
                            return Err(ProtocolError("frame is too short".into()));
                        }
                        SetCommand::exchange(iter)
                    }
                    "PING" => {
                        let msg = if length > 1 {
                            Some(extract_bulk_string(iter.next())?)
                        } else {
                            None
                        };
                        Ok(PING { value: msg })
                    }

                    // 4. 所有其他不认识的命令，都匹配到这里
                    _ => {
                        let args = iter
                            .skip(1)
                            .map(|f| match f {
                                Frame::Bulk(bytes) => Ok(bytes),
                                _ => Err(ProtocolError("命令参数必须是 Bulk String".into())),
                            })
                            .collect::<Result<Vec<_>, _>>()?;

                        Ok(Unimplement {
                            command: command_name,
                            args,
                        })
                    }
                }
            }
            _ => Err(ProtocolError("not a command".into())),
        }
    }
}

/// 尝试从一个 Frame 中提取出 Bulk String 并转换为 String
fn extract_bulk_string(frame: Option<Frame>) -> Result<String, KvError> {
    match frame {
        Some(Frame::Bulk(bytes)) => Ok(String::from_utf8(bytes.to_vec())
            .map_err(|e| ProtocolError(e.to_string()))?
            .to_string()),
        _ => Err(ProtocolError("期望参数是批量字符串".into())),
    }
}

/// 尝试从一个 Frame 中提取出 Bulk String 并转换为 String
fn extract_bulk_integer(frame: Option<Frame>) -> Result<i64, KvError> {
    extract_bulk_string(frame)?.parse::<i64>().map_err(|e|ProtocolError(e.to_string()))
}

/// 尝试从一个 Frame 中提取出 Bulk Bytes
fn extract_bulk_bytes(frame: Option<Frame>) -> Result<Bytes, KvError> {
    match frame {
        Some(Frame::Bulk(bytes)) => Ok(bytes),
        _ => Err(ProtocolError("期望参数是批量字符串".into())),
    }
}
