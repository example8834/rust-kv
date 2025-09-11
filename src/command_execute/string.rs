use bytes::Bytes;

use crate::{command_execute::{calculate_expiration_timestamp_ms, CommandContext, CommandExecutor}, core_execute::str_to_bluk, error::{Command, Expiration, Frame, KvError, SetCommand, ToBulk}};

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
        //这里的self 可以省略么？
        db_lock.set_string(self.key.clone(), self.value.clone(), time_expire);
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
