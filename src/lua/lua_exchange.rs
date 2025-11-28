use bytes::Bytes;
use mlua::{FromLua, IntoLua, Lua, Value};

use crate::error::Frame;

/// 辅助函数：将单个 Lua Value 转换为用于命令参数的 Frame::Bulk
/// 几乎所有东西都被转为字符串/字节。
pub fn lua_value_to_bulk_frame(value: Value<'_>) -> Result<Frame,mlua::Error> {
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
        Value::Table(_)
        | Value::Function(_)
        | Value::Thread(_)
        | Value::UserData(_)
        | Value::LightUserData(_)
        | Value::Error(_) => {
            // 打印具体的类型信息
            eprintln!("CRITICAL DEBUG: Lua 传给 redis.call 的参数类型不对！接收到的参数是: {:?}", value);
            return Err(mlua::Error::runtime("invalid argument type for redis.call"));
        }
    };
    Ok(Frame::Bulk(bytes))
}

impl<'lua> IntoLua<'lua> for Frame {
    fn into_lua(self, lua: &'lua Lua) -> mlua::Result<Value<'lua>> {
        match self {
            // 1. Simple String (状态回复)
            // Redis 习惯把状态回复包装成一个带 ok 字段的表
            // 这样客户端能区分这是 "Simple String" 还是 "Bulk String"
            Frame::Simple(s) => {
                let table = lua.create_table()?;
                table.set("ok", s)?;
                Ok(Value::Table(table))
            }

            // 2. Error (错误回复)
            // 包装成带 err 字段的表，这样 Lua 脚本能捕获到错误对象
            Frame::Error(msg) => {
                let table = lua.create_table()?;
                table.set("err", msg)?;
                Ok(Value::Table(table))
            }

            // 3. Integer (整数)
            // 直接转成 Lua 的整数
            Frame::Integer(i) => Ok(Value::Integer(i as i64)),

            // 4. Bulk String (二进制数据)
            // 直接转成 Lua 的字符串
            Frame::Bulk(data) => Ok(Value::String(lua.create_string(&data)?)),

            // 5. Array (数组)
            // 转成 Lua 的 Table (List)
            // 【注意】Lua 的数组下标是从 1 开始的！
            Frame::Array(frames) => {
                let table = lua.create_table()?;
                for (i, frame) in frames.into_iter().enumerate() {
                    // 递归调用 frame.into_lua(lua)
                    let val = frame.into_lua(lua)?;
                    // 下标 i + 1
                    table.set(i + 1, val)?;
                }
                Ok(Value::Table(table))
            }

            // 6. Null (空值)
            // Redis Lua 环境中，空值会被映射为 boolean false
            // 这虽然很怪，但是是 Redis 的标准行为
            Frame::Null => Ok(Value::Boolean(false)),
        }
    }
}

// 这是一个将 Lua 脚本的【执行结果】转回 Rust Frame 的标准实现
impl<'lua> FromLua<'lua> for Frame {
    fn from_lua(lua_value: Value<'lua>, _lua: &'lua Lua) -> mlua::Result<Self> {
        match lua_value {
            // 1. Lua Nil -> Redis Null Bulk
            Value::Nil => Ok(Frame::Null),

            // 2. Lua String -> Redis Bulk String
            Value::String(s) => {
                // 这里的 s.as_bytes() 获取的是 &[u8]，需要转成 Bytes
                let bytes = Bytes::from(s.as_bytes().to_vec());
                Ok(Frame::Bulk(bytes))
            }

            // 3. Lua Integer -> Redis Integer
            Value::Integer(i) => Ok(Frame::Integer(i as i64)),

            // 4. Lua Number (浮点数) -> Redis Integer (向下取整)
            // Redis 脚本通常把浮点数结果转为整数返回
            Value::Number(n) => Ok(Frame::Integer(n as i64)),

            // 5. Lua Boolean -> Redis 行为很特殊！
            // true -> Integer 1
            // false -> Null (在 Redis Lua 中，false 等同于 nil/null)
            Value::Boolean(b) => {
                if b {
                    Ok(Frame::Integer(1))
                } else {
                    Ok(Frame::Null)
                }
            }

            // 6. Lua Table -> 可能是 Array，也可能是 SimpleString/Error
            Value::Table(t) => {
                // 6.1 检查是否是 {ok: "..."} -> Simple String
                if let Ok(ok_msg) = t.get::<_, String>("ok") {
                    return Ok(Frame::Simple(ok_msg));
                }

                // 6.2 检查是否是 {err: "..."} -> Error
                if let Ok(err_msg) = t.get::<_, String>("err") {
                    return Ok(Frame::Error(err_msg));
                }

                // 6.3 否则当作普通 Array 处理
                // sequence_values 会自动处理 1, 2, 3... 的连续下标
                let mut frames = Vec::new();
                for val in t.sequence_values::<Value>() {
                    let v = val?;
                    // 递归调用 from_lua
                    frames.push(Frame::from_lua(v, _lua)?);
                }
                Ok(Frame::Array(frames))
            }

            _ => Err(mlua::Error::runtime(format!(
                "Lua result type {:?} cannot be converted to Redis Frame",
                lua_value
            ))),
        }
    }
}
