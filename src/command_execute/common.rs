use bytes::Bytes;

use crate::{command_execute::{CommandContext, CommandExecutor}, error::{EvalCommand, Frame, KvError, PingCommand, UnimplementCommand}, lua::lua_vm::lua_vm_redis_call};

impl CommandExecutor for PingCommand {
    async fn execute<'ctx>(self, _ctx:  CommandContext<'ctx>) -> Result<Frame, KvError> {
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
        _ctx:  CommandContext<'ctx>
    ) -> Result<Frame, KvError> {
        Ok(Frame::Error(format!("ERR unknown command '{}'", self.command)))
    }
}

/*
 这个是比较特殊的执行层
 */
impl CommandExecutor for EvalCommand {
    async fn execute<'ctx>(
        self,
        ctx:  CommandContext<'ctx>,
    ) -> Result<Frame, KvError>   {
        let command_context = ctx.command_context.unwrap();
        lua_vm_redis_call(command_context.receivce_lua,ctx.db.clone(),command_context.lua_handle);
        Ok(())
    }
}