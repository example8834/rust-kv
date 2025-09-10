use std::process::Command;
 mod string;
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

pub trait CommandExchange {
    async fn exchange(self, itor: IntoIter<Frame>) -> Result<Command, KvError>;
}