use std::ops::Index;

use crate::error::Command::{Get, PING, Set, Unimplement};
use crate::error::KvError::ProtocolError;
use crate::error::{Command, Expiration, Frame, KvError, SetCondition};
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
        let mut iter = frames.into_iter();
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
                        let get_key = extract_bulk_string(iter.next())?;
                        Ok(Get { key: get_key })
                    }
                    "SET" => {
                        if length < 3 {
                            return Err(ProtocolError("frame is too short".into()));
                        }
                        let set_key = extract_bulk_string(iter.next())?;
                        let set_value = extract_bulk_bytes(iter.next())?;
                        let mut expiration: Option<Expiration> = None;
                        let mut condition: Option<SetCondition> = None;
                        let mut index = 3;
                        while index < length - 1 {
                            let frame_str = extract_bulk_string(iter.next())?.to_uppercase();
                            match frame_str.as_str() {
                                "EX" => {
                                    // EX 后面必须跟一个数字
                                    index += 1; // 移动到下一个参数
                                    if index >= length {
                                        return Err(ProtocolError("😧 EX 后参数传递错误".into()));
                                    }
                                    let seconds = extract_bulk_integer(iter.next())?;
                                    expiration = Some(Expiration::EX(seconds as u64 ));
                                }
                                "PX" => {
                                    // PX 后面也必须跟一个数字
                                    index += 1;
                                    if index >= length {
                                        return Err(ProtocolError("😧 PX 后参数传递错误".into()));
                                    }
                                    let ms = extract_bulk_integer(iter.next())?;
                                    expiration = Some(Expiration::PX(ms as u64));
                                }
                                "NX" => {
                                    // NX 和 XX 不能同时存在
                                    if condition.is_some() {
                                        return Err(ProtocolError("😧 NX 和 XX 不能同时存在".into()));
                                    }
                                    condition = Some(SetCondition::NX);
                                }
                                "XX" => {
                                    // NX 和 XX 不能同时存在
                                    if condition.is_some() {
                                        return Err(ProtocolError("😧 NX 和 XX 不能同时存在".into()));
                                    }
                                    condition = Some(SetCondition::XX);
                                }
                                _ => {
                                    // 遇到了无法识别的选项
                                    return Err(ProtocolError("😧 碰到无法识别错误".into()));
                                }
                            }
                        }
                        Ok(Set {
                            key: set_key,
                            value: set_value,
                            expiration:expiration,
                            conditiion:condition
                        })
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
