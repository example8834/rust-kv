use bytes::Bytes;

use crate::{
    aof_exchange::{exchange_absolute_time, parse_int_from_bytes, CommandAofExchange}, command_execute::CommandContext, core_time::get_cached_time_ms, error::{Frame, KvError, SetCommand}
};

impl CommandAofExchange for SetCommand {
    async fn execute_aof<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx:  CommandContext<'ctx>
    ) -> Result<Frame, KvError> {
        if ctx.command_context.is_none(){
            return Ok(Frame::Simple("OK".to_string()));
        }
        let mut frame_vec = vec![crate::error::Frame::Bulk(Bytes::from("SET".to_string()))];
        frame_vec.push(crate::error::Frame::Bulk(Bytes::from(self.key.to_string())));
        frame_vec.push(crate::error::Frame::Bulk(Bytes::from(self.value)));
        if let Some(expire) = self.expiration {
            match expire {
                crate::error::Expiration::EX(s) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("EXAT".to_string())));
                    let expire_bytes = exchange_absolute_time(s * 1000);
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
                crate::error::Expiration::PX(ms) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("PXAT".to_string())));
                    let expire_bytes = exchange_absolute_time(ms);
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
                crate::error::Expiration::EXAT(s) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("EXAT".to_string())));
                    let expire_bytes = parse_int_from_bytes(s);
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                },
                crate::error::Expiration::PXAT(ms) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("PXAT".to_string())));
                    let expire_bytes = parse_int_from_bytes(ms);
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                },
            }
        }
        if let Some(sender) = ctx.command_context {
            if let Err(e) = sender.aof_tx.send(Frame::Array(frame_vec).serialize()).await {
                eprintln!("发送AOF消息失败: {}", e);
            }
            Ok(Frame::Simple("OK".to_string()))
        } else {
            Ok(Frame::Simple("OK".to_string()))
        }
    }
}
