use bytes::Bytes;

use crate::{command_execute::{CommandContext, CommandExecutor}, error::{Frame, KvError, PingCommand, UnimplementCommand}};

impl CommandExecutor for PingCommand {
    async fn execute<'ctx>(self, _ctx: &'ctx mut CommandContext<'ctx>) -> Result<Frame, KvError> {
        if let Some(value) = &self.value {
            Ok(Frame::Bulk(Bytes::from(value.clone())))
        } else {
            Ok(Frame::Simple("PONG".into()))
        }
    }
} 


impl CommandExecutor for UnimplementCommand {
    async fn execute<'ctx>(
        self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        _ctx: &'ctx mut CommandContext<'ctx>
    ) -> Result<Frame, KvError> {
        Ok(Frame::Error(format!("ERR unknown command '{}'", self.command)))
    }
}