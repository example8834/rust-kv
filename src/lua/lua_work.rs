use mlua::prelude::*;
use std::{cell::RefCell, collections::HashMap, sync::Arc, thread};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::{
    command_execute::CommandContext,
    context::{CONN_STATE, ConnectionContent, ConnectionState},
    db::{Db, LockedDb},
    error::{EvalCommand, Frame, KvError},
    lua::lua_vm::{general_lua, init_lua_pre},
};

// 一个请求包含：上下文参数 + 回信地址
pub struct LuaTask {
    pub ctx: CommandContext, // 你的参数
    pub resp: oneshot::Sender<Result<Frame, KvError>>,
    pub command: EvalCommand,
    pub connect_state: ConnectionState,
}

// 这里啊是为了提前操作lua 所以需要的结构
pub struct CurrentRequestEnv {
    pub ctx: CommandContext, // 你的环境            // 你的环境
    pub sessions: Arc<Mutex<HashMap<usize, LockedDb>>>, // 你的锁 (直接用 HashMap，不需要 Arc Mutex)
    pub command:  EvalCommand,
}

thread_local! {
    pub static CURRENT_ENV: RefCell<Option<CurrentRequestEnv>> = RefCell::new(None);
}

#[derive(Clone)]
pub struct LuaRouter {
    // 存放所有工人的通道
    senders: Vec<mpsc::Sender<LuaTask>>,
}

impl LuaRouter {
    // 【核心修改】智能分发：找最闲的那个线程
    pub async fn dispatch(&self, task: LuaTask) -> Result<(), KvError> {
        // 1. 寻找剩余容量最大的那个通道索引 (Least Queue Depth)
        //    max_by_key 会遍历 Vec，找到 capacity() 最大的那个
        //    对于 8~16 个线程来说，这个循环非常快
        let mut best_index = 0;
        let mut max_capacity = 0;

        for (i, sender) in self.senders.iter().enumerate() {
            let cap = sender.capacity();
            // 如果发现有空闲容量更大的，记录下来
            if cap > max_capacity {
                max_capacity = cap;
                best_index = i;
            }
        }

        // 小优化：如果所有通道都满了 (capacity 都是 0)，那就退化成随机或者轮询
        // 不过这里我们直接硬发给 best_index，让它去排队等待（Tokio 会处理背压）

        // 2. 发送任务给选中的“幸运儿”
        self.senders[best_index]
            .send(task)
            .await
            .map_err(|_| KvError::ProtocolError("Lua 引擎过载或已关闭".into()))
    }
}
// 启动一个 Lua 专用线程，返回它的“传菜口”(Sender)
// 启动函数稍微调整一下，不再需要 AtomicUsize 了
pub fn start_multi_lua_actor(worker_num: usize, queue_size: usize) -> LuaRouter {
    let mut senders = Vec::with_capacity(worker_num);

    for i in 0..worker_num {
        let (tx, mut rx) = mpsc::channel::<LuaTask>(queue_size);
        senders.push(tx);

        thread::spawn(move || {
            //独占环境
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            //单线程声明
            let local = tokio::task::LocalSet::new();

            println!("Lua Worker #{} (智能负载) 启动就绪", i);

            //这里才开始执行单线程内容
            local.block_on(&rt, async move {
                //生成lua 初始化内部内容 开始绑定
                let lua = general_lua().await.unwrap();
                while let Some(task) = rx.recv().await {
                    let sender = task.resp;
                    let result = CONN_STATE
                        .scope(task.connect_state, async {
                            init_lua_pre(&lua, &task.command,task.ctx).await;
                            EvalCommand::lua_vm_redis_call(&(task.command), &lua).await
                        })
                        .await;
                    let _ = sender.send(result);
                }
            });
        });
    }

    LuaRouter {
        senders,
        // counter 删掉了，不需要了
    }
}
