use std::thread;
use tokio::sync::{mpsc, oneshot};
use mlua::prelude::*;

use crate::{command_execute::CommandContext, error::{EvalCommand, Frame, KvError}};

// 一个请求包含：上下文参数 + 回信地址
pub struct LuaTask{
    pub ctx: CommandContext, // 你的参数
    pub resp: oneshot::Sender<Result<Frame, KvError>>, 
    pub command:EvalCommand
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
                // --- 这里是你原本的逻辑 ---
                // 注意：这里不需要再 spawn 了，直接跑！
                // 这里的 run_lua_logic 就是你那个 lua_vm_redis_call
                let result = EvalCommand::lua_vm_redis_call(command,task.ctx).await; 
                // -----------------------

                // 做完了，把菜传回前台
                let _ = sender.send(result);
            }
        });
    });

    tx // 把传菜口返回给主程序
}
