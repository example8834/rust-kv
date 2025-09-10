use crate::{command_execute::CommandExecutor, error::Command};

impl CommandExecutor for Set {
    async fn execute(self, ctx: &mut super::CommandContext) -> Result<Frame, KvError> {
        let db_lock = ctx.db.lock().await;
        //这里的self 可以省略么？
        db_lock.set_string(key, value, time_expire);
        //序列化问题
        if let Some(sender) = ctx.tx {
            //这个是序列化
            let mut frame_vec = vec![
                Frame::Bulk(Bytes::from("SET")),
                // 这里 key 和 value 是 move 进来的，不再需要 clone
                Frame::Bulk(Bytes::from(key.clone())),
                Frame::Bulk(value.clone()),
            ];
            if let Some(expire) = &expiration {
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

            if let Some(condition) = conditiion {
                match condition {
                    crate::error::SetCondition::NX => {
                        frame_vec.push(str_to_bluk(ToBulk::String("NX".into())));
                    }
                    crate::error::SetCondition::XX => {
                        frame_vec.push(str_to_bluk(ToBulk::String("XX".into())));
                    }
                }
            }
            sender.send(Frame::Array(frame_vec.clone()).serialize())
        };
        Ok(Frame::Simple("OK".to_string()))
    }
}
