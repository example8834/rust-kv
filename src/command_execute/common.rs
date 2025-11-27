use bytes::Bytes;

use crate::{
    command_execute::{CommandContext, CommandExecutor},
    db::LockedDb,
    error::{EvalCommand, Frame, KvError, PingCommand, UnimplementCommand}, lua::lua_work::LuaTask,
};
use tokio::sync::oneshot;
impl CommandExecutor for PingCommand {
    async fn execute(
        &self,
        _ctx: CommandContext
        ,db_lock: Option<& mut LockedDb>
    ) -> Result<Frame, KvError> {
        if let Some(value) = &self.value {
            Ok(Frame::Bulk(Bytes::from(value.clone())))
        } else {
            Ok(Frame::Simple("PONG".into()))
        }
    }
}

impl CommandExecutor for UnimplementCommand {
    async fn execute(
        &self,
        // 2. 将这个生命周期 'ctx 应用到 CommandContext 的引用上
        _ctx: CommandContext
        ,db_lock: Option<& mut LockedDb>
    ) -> Result<Frame, KvError> {
        Ok(Frame::Error(format!(
            "ERR unknown command '{}'",
            self.command
        )))
    }
}

/*
这个是比较特殊的执行层
*/
impl CommandExecutor for EvalCommand {
    async fn execute(
        &self,
        ctx: CommandContext,
        _db_lock: Option<& mut LockedDb>
    ) -> Result<Frame, KvError> {
    //   let result =   self.lua_vm_redis_call(
    // CommandContext { 
    //     db: ctx.db.clone(), 
    //     connect_content: ctx.connect_content.clone(), 
    // }).await; // 直接 await！
    //现在我复制了这个链接
    let a = ctx.connect_content.clone().unwrap().clone();

    // 这里的 Result<Frame, KvError> 就是你要通过信封回传的数据类型
    let (tx, rx) = oneshot::channel::<Result<Frame, KvError>>();

    a.lua_sender.send(LuaTask{
        ctx:ctx.clone(),
        resp: tx,
        command: self.clone(),
    });

    let result = rx.await.unwrap();
    result
    }
}
