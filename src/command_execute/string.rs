use bytes::Bytes;

use crate::{
    aof_exchange::CommandAofExchange, command_execute::{
        bytes_to_i64_fast, calculate_expiration_timestamp_ms, parse_int_from_bytes, CommandContext, CommandExecutor
    }, error::{Command, Expiration, Frame, GetCommand, KvError, SetCommand, ToBulk}, types::{Element, Value, ValueEntry}
};

impl CommandExecutor for SetCommand {
    // 必须在这里也加上 <'ctx> 和对应的生命周期标注
    async fn execute<'ctx>(self, ctx: &'ctx CommandContext<'ctx>) -> Result<Frame, KvError> {
        
        let time_expire_u64: Option<u64>;
        let time_expire = if let Some(expire) = &self.expiration {
            time_expire_u64 = Some(calculate_expiration_timestamp_ms(expire));
            time_expire_u64
        } else {
            None
        };
        let value_obj;
        match bytes_to_i64_fast(&self.value) {
            Some(i) => {
                value_obj = ValueEntry {
                    data: Value::Simple(Element::Int(i)),
                    expires_at: time_expire,
                    eviction_metadata: todo!(),
                }
            }
            None => {
                value_obj = ValueEntry {
                    data: Value::Simple(Element::String(self.value)),
                    expires_at: time_expire,
                    eviction_metadata: todo!(),
                }
            }
        };
        //获取之后立刻使用。减少锁持有时间
        let mut db_lock = ctx.db.lock_write().await;
        //这里的self
        db_lock.set_string(self.key, value_obj);
        //序列化问题
        self.execute_aof(ctx).await?;
        Ok(Frame::Simple("OK".to_string()))
    }
    
}

impl CommandExecutor for GetCommand {
    async fn execute<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: &'ctx  CommandContext<'ctx>,
    ) -> Result<Frame, KvError> {
        let db_lock = ctx.db.lock_read().await;
        let value = db_lock.get_string(self.key);
        match value {
            Some(entry) => {
                let data = entry.data;

                //这是处理字符串的方法
                match data {
                    Value::Simple(Element::String(bytes)) => {
                        Ok(Frame::Bulk(bytes))
                    }
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
