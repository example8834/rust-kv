use crate::error::Frame::{Array, Bulk};
use crate::error::KvError::ProtocolError;
use crate::error::{Command, Frame, KvError};
use bytes::{Buf, Bytes, BytesMut};
use memchr::memmem;
use std::io::{Cursor, stdout};
/// --- 2. 核心解析逻辑 ---

/// 总调度函数：尝试从可变的 BytesMut 缓冲区解析一个 Frame。
/// 这是暴露给外部的唯一入口。
pub fn parse_frame(buf: &[u8]) -> Result<Option<(Frame,usize)>, KvError> {
    // 1. 创建一个 Cursor 来进行安全的“只读预演”。
    //    Cursor 包裹的是一个对 buf 数据的只读切片。
    let mut cursor = Cursor::new(&buf[..]);

    // 2. 在 Cursor 上进行递归解析。
    //    这个过程不会修改原始的 buf。
    match parse_frame_from_cursor(&mut cursor)? {
        Some(frame) => {
            // 3. 如果“预演”成功，我们通过 cursor.position() 知道了总共消耗了多少字节。
            let consumed = cursor.position() as usize;
            // 4. 才进行唯一一次的、破坏性的操作：从原始 buf 中消耗掉这些字节。
            Ok(Some((frame,consumed)))
        }
        // 如果预演时发现数据不完整 (Ok(None)) 或格式错误 (Err)，
        _ => Ok(None)
    }
}

/// 在 Cursor 上进行递归解析的“真正”核心函数。
/// 它只在只读的数据上操作，不修改任何东西。
fn parse_frame_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Option<Frame>, KvError> {
    // 检查游标后面是否还有数据可读
    if !cursor.has_remaining() {
        return Ok(None);
    }
    println!("{:?}",std::str::from_utf8(cursor.get_ref()));
    // 根据游标当前位置的第一个字节来决定如何解析
    match cursor.get_ref()[cursor.position() as usize] {
        b'*' => parse_array_from_cursor(cursor),
        b'$' => parse_bulk_string_from_cursor(cursor),
        _ => Err(ProtocolError("无效的 Frame 类型前缀".into()))
    }
}

/// 在 Cursor 上解析数组
fn parse_array_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Option<Frame>, KvError> {
    // 1. 从 Cursor 当前位置读取一行元数据 (e.g., "*2\r\n")
    if let Some(line_bytes) = read_line_from_cursor(cursor)? {
        if line_bytes[0] != b'*' {
            return Err(ProtocolError("期望是数组 '*'".into()));
        }

        let num_elements = parse_decimal(&line_bytes[1..])?;

        let mut elements = Vec::with_capacity(num_elements);
        // 2. 循环 N 次，递归地在 Cursor 上解析子元素
        for _ in 0..num_elements {
            if let Some(child_frame) = parse_frame_from_cursor(cursor)? {
                elements.push(child_frame);
            } else {
                // 如果任何一个子元素不完整，则整个数组都不完整
                return Ok(None);
            }
        }
        Ok(Some(Frame::Array(elements)))
    } else {
        // 元数据行都不完整
        Ok(None)
    }
}

/// 在 Cursor 上解析批量字符串
fn parse_bulk_string_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Option<Frame>, KvError> {
    if let Some(line_bytes) = read_line_from_cursor(cursor)? {
        if line_bytes[0] != b'$' {
            return Err(ProtocolError("期望是批量字符串 '$'".into()));
        }
        let data_len = parse_decimal(&line_bytes[1..])?;

        // 检查游标后面“剩下”的数据是否足够
        if cursor.remaining() < data_len + 2 - 1 {
            return Ok(None);
        }

        // 提取数据
        let data_start = cursor.position() as usize;
        let data_end = data_start + data_len;
        let data = Bytes::copy_from_slice(&cursor.get_ref()[data_start..data_end]);
        // 移动游标，跳过数据
        cursor.advance(data_len);

        // 检查并消耗结尾的 \r\n
        if &cursor.get_ref()[cursor.position() as usize .. cursor.position() as usize + 2] != b"\r\n" {
            return Err(ProtocolError("批量字符串结尾缺少 \\r\\n".into()));
        }
        cursor.advance(2);

        Ok(Some(Bulk(data)))
    } else {
        // 元数据行都不完整
        Ok(None)
    }
}

// --- 3. 辅助函数 ---

/// 核心辅助函数：从 Cursor 当前位置读取一行，并移动 Cursor 的位置指针
fn read_line_from_cursor<'a>(cursor: &mut Cursor<&'a [u8]>) -> Result<Option<&'a [u8]>, KvError> {
    let start = cursor.position() as usize;
    let end = cursor.get_ref().len();
    let remaining_buf = &cursor.get_ref()[start..end];
    if let Some(crlf_pos) = find_crlf(remaining_buf) {
        let line_bytes = &remaining_buf[..crlf_pos];
        cursor.advance(crlf_pos + 2);
        Ok(Some(line_bytes))
    } else {
        Ok(None)
    }
}

/// 在字节切片中查找 CRLF (`\r\n`)
// fn find_crlf(buf: &[u8]) -> Option<usize> {
//     buf.windows(2).position(|window| window == b"\r\n")
// }


/// 在字节切片中查找 CRLF (`\r\n`)，使用 memchr 进行 SIMD 优化
fn find_crlf(buf: &[u8]) -> Option<usize> {
    // 创建一个针对 b"\r\n" 的专用查找器
    // Finder::new 的开销很小，可以在循环中重复创建
    let finder = memmem::Finder::new(b"\r\n");
    finder.find(buf)
}


/// 将字节切片解析成一个 usize 类型的十进制数
fn parse_decimal(bytes: &[u8]) -> Result<usize, KvError> {
    let s = std::str::from_utf8(bytes)
        .map_err(|_| ProtocolError("无效的 UTF-8 数字序列".into()))?;
    s.parse::<usize>()
        .map_err(|_| ProtocolError("无效的十进制格式".into()))
}

