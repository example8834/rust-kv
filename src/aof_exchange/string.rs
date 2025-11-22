use bytes::Bytes;
use futures::SinkExt;

use crate::{
    aof_exchange::{AofContent, CommandAofExchange, exchange_absolute_time, parse_int_from_bytes},
    command_execute::CommandContext,
    core_time::get_cached_time_ms,
    error::{Frame, KvError, SetCommand},
};

impl CommandAofExchange for SetCommand {
    async fn execute_aof<'a>(
        &self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        ctx: AofContent<'a>,
    ) {
        let mut frame_vec = vec![crate::error::Frame::Bulk(Bytes::from("SET".to_string()))];
        frame_vec.push(crate::error::Frame::Bulk(Bytes::from(self.key.to_string())));
        frame_vec.push(crate::error::Frame::Bulk(Bytes::from(self.value.clone())));
        if let Some(expire) = &self.expiration {
            match expire {
                crate::error::Expiration::EX(s) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("EXAT".to_string())));
                    let expire_bytes = exchange_absolute_time(s * 1000);
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
                crate::error::Expiration::PX(ms) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("PXAT".to_string())));
                    let expire_bytes = exchange_absolute_time(ms.clone());
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
                crate::error::Expiration::EXAT(s) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("EXAT".to_string())));
                    let expire_bytes = parse_int_from_bytes(s.clone());
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
                crate::error::Expiration::PXAT(ms) => {
                    frame_vec.push(crate::error::Frame::Bulk(Bytes::from("PXAT".to_string())));
                    let expire_bytes = parse_int_from_bytes(ms.clone());
                    frame_vec.push(crate::error::Frame::Bulk(expire_bytes));
                }
            }
        }
        if let Err(e) = ctx.aof_tx.send(Frame::Array(frame_vec).serialize()).await {
            eprintln!("发送AOF消息失败: {}", e);
        };
        ()
    }
}
