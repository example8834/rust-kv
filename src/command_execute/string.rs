use bytes::Bytes;
use tracing_subscriber::util;

use crate::{
    aof_exchange::CommandAofExchange, command_execute::{
        CommandContext, CommandExecutor, bytes_to_i64_fast, calculate_expiration_timestamp_ms,
        parse_int_from_bytes,
    }, db::LockedDb, error::{Command, Expiration, Frame, GetCommand, KvError, SetCommand, ToBulk}, types::{Element, Value, ValueEntry}
};

impl CommandExecutor for SetCommand {
    // 必须在这里也加上 <'ctx> 和对应的生命周期标注
    async fn execute(&self, ctx: CommandContext,db_lock: Option<& mut LockedDb>) -> Result<Frame, KvError> {
        let time_expire_u64: Option<u64>;
        let time_expire = if let Some(expire) = &self.expiration {
            time_expire_u64 = Some(calculate_expiration_timestamp_ms(expire));
            time_expire_u64
        } else {
            None
        };
        //再这里创建value
        let value_obj = match bytes_to_i64_fast(&self.value) {
            Some(i) => ValueEntry::new(Value::Simple(Element::Int(i)), time_expire),
            None => ValueEntry::new(Value::Simple(Element::String(self.value.clone())), time_expire),
        };
        //ctx.db_lock.unwrap().set_string(self.key.clone(), value_obj);
        if let Some(LockedDb::Write(  map)) = db_lock {
            map.insert(self.key.clone(), value_obj);
        }
        Ok(Frame::Simple("OK".to_string()))
    }
}

impl CommandExecutor for GetCommand {
    async fn execute(
        &self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: CommandContext,
        db_lock: Option<& mut LockedDb>
    ) -> Result<Frame, KvError> {
        let value= if let Some(LockedDb::Read( map)) = db_lock {
            map.select(&self.key.clone())
        }else {
            None
        };
        match value {
            Some(entry) => {
                let data = entry.data.clone();

                //这是处理字符串的方法
                match data {
                    Value::Simple(Element::String(bytes)) => Ok(Frame::Bulk(bytes)),
                    //性能优化
                    Value::Simple(Element::Int(i)) => {
                        let bytes = parse_int_from_bytes(i);
                        Ok(Frame::Bulk(Bytes::from(bytes)))
                    }
                    _ => Ok(Frame::Null), // 如果不是字符串类型，返回 Null
                }
            }
            None => Ok(Frame::Null),
        }
    }
}
