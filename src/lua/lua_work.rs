use mlua::prelude::*;
use std::thread;
use tokio::sync::{mpsc, oneshot};

use crate::{
    command_execute::CommandContext,
    context::{CONN_STATE, ConnectionContent, ConnectionState},
    db::Db,
    error::{EvalCommand, Frame, KvError},
};

// 一个请求包含：上下文参数 + 回信地址
pub struct LuaTask {
    pub ctx: CommandContext, // 你的参数
    pub resp: oneshot::Sender<Result<Frame, KvError>>,
    pub command: EvalCommand,
    pub connect_state: ConnectionState,
}

// 启动一个 Lua 专用线程，返回它的“传菜口”(Sender)
pub fn start_lua_actor<'ctx>() -> mpsc::Sender<LuaTask> {
    // 建立通道：缓冲区 100
    let (tx, mut rx) = mpsc::channel::<LuaTask>(100);

    // 【重点】开一个独立的 OS 线程
    thread::spawn(move || {
        // 1. 在这里创建 Lua，它是地头蛇，永远不出这个线程
        let lua = Lua::new();

        // 2. 因为你要跑 async lua，必须在这里建一个单线程 Runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        // 3. 必须用 LocalSet，这是允许 !Send Future 存在的关键
        let local = tokio::task::LocalSet::new();

        // 开始死循环干活
        local.block_on(&rt, async move {
            while let Some(task) = rx.recv().await {
                //这是通知的关键
                let sender = task.resp;
                //这是命令
                let command = &task.command;

                // =====================================================
                // 2. 【套上 Scope】并执行
                //    注意：scope(...).await 的结果，就是内部 async 块的返回值
                // =====================================================
                let result = CONN_STATE
                    .scope(task.connect_state, async move {
                        // 在这个花括号里，CONN_STATE 是有效的！
                        // lock_write_lua 调用 with() 就不会 Panic 了

                        EvalCommand::lua_vm_redis_call(command, task.ctx).await
                    })
                    .await; // <--- 这里的 await 会拿到内部函数的返回值
                // 做完了，把菜传回前台
                let _ = sender.send(result);
            }
        });
    });

    tx // 把传菜口返回给主程序
}
