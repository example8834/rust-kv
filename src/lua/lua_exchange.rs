use bytes::Bytes;
use mlua::Value;

use crate::error::Frame;

/// 辅助函数：将单个 Lua Value 转换为用于命令参数的 Frame::Bulk
/// 几乎所有东西都被转为字符串/字节。
pub fn lua_value_to_bulk_frame(value: Value<'_>) -> Result<Frame, mlua::Error> {
    let bytes = match value {
        // Lua nil 作为参数时，等同于空字符串
        Value::Nil => Bytes::new(),
        
        // 布尔值转为 "1" 或 "0"
        Value::Boolean(b) => Bytes::from(if b { "1" } else { "0" }),
        
        // 整数转为字符串
        Value::Integer(i) => Bytes::from(i.to_string()),
        
        // 浮点数转为字符串
        Value::Number(n) => Bytes::from(n.to_string()),
        
        // 字符串（可能是非UTF-8）转为 Bytes
        Value::String(s) => Bytes::from(s.as_bytes().to_vec()),
        
        // 其他类型不能作为 redis.call 的参数
        Value::Table(_) | 
        Value::Function(_) | 
        Value::Thread(_) | 
        Value::UserData(_) |
        Value::LightUserData(_) |
        Value::Error(_) => {
            return Err(mlua::Error::runtime("invalid argument type for redis.call"))
        }
    };
    Ok(Frame::Bulk(bytes))
}