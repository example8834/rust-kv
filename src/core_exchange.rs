use crate::error::Command::{Get, Set, Unimplement, PING};
use crate::error::KvError::{Io, ProtocolError};
use crate::error::{Command, Frame, KvError};
use bytes::Bytes;

impl TryFrom<Frame> for Command {
    type Error = KvError;

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        let frames = match frame {
            Frame::Array(frames) => frames,
            _ => return Err(ProtocolError("must be a array from frame".into())),
        };

        match frames.get(0) {
            Some(Frame::Bulk(start_str)) => {
                let command_name = std::str::from_utf8(start_str)
                    .map_err(|_| ProtocolError("zhuan huan yi chang ".into()))?
                    .to_uppercase();
                match command_name.as_str() {
                    "GET" => {
                        if frames.len() != 2 {
                            return Err(ProtocolError("GET 命令需要 1 个参数".into()));
                        }
                        let get_key = extract_bulk_string(&frames[1])?;
                        Ok(Get { key: get_key })
                    }
                    "SET" => {
                        if frames.len() != 3 {
                            return Err(ProtocolError("frame is too short".into()));
                        }
                        let set_key = extract_bulk_string(&frames[1])?;
                        let set_value = extract_bulk_bytes(&frames[2])?;
                        Ok(Set {
                            key: set_key,
                            value: set_value,
                        })
                    }
                    "PING" =>{
                        let msg = if frames.len() > 1 {
                            Some(extract_bulk_string(&frames[1])?)
                        } else {
                            None
                        };
                        Ok(PING {
                            value: msg,
                        })
                    }
                    // 4. 所有其他不认识的命令，都匹配到这里
                    _ => {
                        let args = frames.into_iter().skip(1).map(|f| {
                            match f {
                                Frame::Bulk(bytes) => Ok(bytes),
                                _ => Err(ProtocolError("命令参数必须是 Bulk String".into()))
                            }
                        }).collect::<Result<Vec<_>, _>>()?;

                        Ok(Unimplement { command: command_name, args })
                    }
                }
            }
            _ => Err(ProtocolError("not a command".into())),
        }
    }
}

/// 尝试从一个 Frame 中提取出 Bulk String 并转换为 String
fn extract_bulk_string(frame: &Frame) -> Result<String, KvError> {
    match frame {
        Frame::Bulk(bytes) => Ok(std::str::from_utf8(bytes)
            .map_err(|e| ProtocolError(e.to_string()))?
            .to_string()),
        _ => Err(ProtocolError("期望参数是批量字符串".into())),
    }
}

/// 尝试从一个 Frame 中提取出 Bulk Bytes
fn extract_bulk_bytes(frame: &Frame) -> Result<Bytes, KvError> {
    match frame {
        Frame::Bulk(bytes) => Ok(bytes.clone()),
        _ => Err(ProtocolError("期望参数是批量字符串".into())),
    }
}
