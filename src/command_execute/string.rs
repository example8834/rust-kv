use bytes::Bytes;

use crate::{command_execute::{bytes_to_i64_fast, calculate_expiration_timestamp_ms, monotonic_time_ms, parse_int_from_bytes, CommandContext, CommandExecutor}, core_execute::str_to_bluk, db::{Element, ValueEntry}, error::{Command, Expiration, Frame, GetCommand, KvError, SetCommand, ToBulk}};

impl CommandExecutor for SetCommand {
     // 必须在这里也加上 <'ctx> 和对应的生命周期标注
    async fn execute<'ctx>(
        self,
        ctx: &'ctx mut CommandContext<'ctx>
    ) -> Result<Frame, KvError> {
        let mut db_lock = ctx.db.lock().await;
        let time_expire = if let Some(expire) = &self.expiration {
            Some(calculate_expiration_timestamp_ms(expire))
        }else{
            None
        };
        let value_obj;
        match bytes_to_i64_fast(&self.value) {
            Some(i) => {
                value_obj = ValueEntry {
                    data: crate::db::Value::Simple(Element::Int(i)),
                    expires_at: time_expire,
                }
            }
            None => {
                value_obj = ValueEntry {
                    data: crate::db::Value::Simple(Element::String(self.value.clone())),
                    expires_at: time_expire,
                }
            }
        };
        //这里的self 可以省略么？
        db_lock.set_string(self.key.clone(), value_obj, time_expire);
        //序列化问题
        if let Some(sender) = ctx.tx {
            //这个是序列化
            let mut frame_vec = vec![
                Frame::Bulk(Bytes::from("SET")),
                // 这里 key 和 value 是 move 进来的，不再需要 clone
                Frame::Bulk(Bytes::from(self.key)),
                Frame::Bulk(self.value),
            ];
            if let Some(expire) = &self.expiration {
                match expire {
                    Expiration::PX(time) => {
                        frame_vec.push(str_to_bluk(ToBulk::String("PX".into())));
                        frame_vec.push(str_to_bluk(ToBulk::String(time.to_string())));
                    }
                    Expiration::EX(time) => {
                        frame_vec.push(str_to_bluk(ToBulk::String("EX".into())));
                        frame_vec.push(str_to_bluk(ToBulk::String(time.to_string())));
                    }
                }
            }

            if let Some(condition) = self.condition {
                match condition {
                    crate::error::SetCondition::NX => {
                        frame_vec.push(str_to_bluk(ToBulk::String("NX".into())));
                    }
                    crate::error::SetCondition::XX => {
                        frame_vec.push(str_to_bluk(ToBulk::String("XX".into())));
                    }
                }
            }
            sender.send(Frame::Array(frame_vec.clone()).serialize()).await;
        }
        Ok(Frame::Simple("OK".to_string()))
    }
}


impl CommandExecutor for GetCommand  {
    async fn execute<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: &'ctx mut CommandContext<'ctx>
    ) -> Result<Frame, KvError> {
        let db_lock = ctx.db.lock().await;
        let value = db_lock.get_string(&self.key);
        match value {
            Some(entry) => {
                let data = entry.data;

                //这是处理字符串的方法
                match data {
                    crate::db::Value::Simple(crate::db::Element::String(bytes)) => Ok(Frame::Bulk(bytes)),
                    //性能优化
                    crate::db::Value::Simple(crate::db::Element::Int(i)) => {
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