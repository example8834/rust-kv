use std::{collections::HashMap, f32::consts::E, sync::Arc};

use flume::Sender;
use mlua::Lua;
use tokio::{
    runtime::{Handle, Runtime},
    sync::Mutex,
};

use crate::{
    command_execute::CommandContext, context::ConnectionState, core_execute::execute_command_hook, db::{
        LockedDb,
        eviction::{KvOperator, MemoryCache},
    }, error::{Command, EvalCommand, Frame, KvError}, lua::{lua_exchange::lua_value_to_bulk_frame, lua_work::{CURRENT_ENV, CurrentRequestEnv}}
};

impl EvalCommand {
    /*
    处理lua 的vm 的脚本执行方法
    */
    pub async fn lua_vm_redis_call(&self, lua: &Lua) -> Result<Frame, KvError> {
        let script = self.script.clone();

        //预期执行
        let result: Result<Frame, mlua::Error> = lua.load(script).eval_async::<Frame>().await;
        //在执行之后 session 肯定有了
        let sessions = CURRENT_ENV
            .with(|cell| {
                // 1. 获取写锁 (RefMut<Option<CurrentRequestEnv>>)
                let borrow = cell.borrow_mut();
                borrow.as_ref().unwrap().sessions.clone()
            });
        // 【在这里】统一进行错误类型转换
        let final_result: Result<Frame, KvError> = result.map_err(|e| {
            // 把 mlua 的错误转成你的 KvError
            // 这样你的系统内部就统一了
            KvError::ProtocolError(format!("Lua脚本错误: {}", e))
        });
        let mut entry = sessions.lock().await;
        //如果没有问题就提交
        if final_result.is_ok() {
            //拿出arc 的所有权 并且全部提交
            for (_size, lock) in entry.drain() {
                if let LockedDb::Write(lock_mut) = lock {
                    lock_mut.as_transactional().unwrap().commit().await;
                }
            }
        }
        final_result
    }
}
pub async fn general_lua() -> Result<Lua, KvError> {
    let lua = Lua::new();
    let redis_call = lua
        .create_async_function(
            // 关键改变在这里！我们只接收一个 `args`，它包含了所有参数！
            move |_lua, args: mlua::MultiValue| {
                //在第一次调用的时候才初始化 但是顺序上没有问题
                let (sessions, db_clone, content) = CURRENT_ENV
                    .with(|cell| {
                        // 1. 获取写锁 (RefMut<Option<CurrentRequestEnv>>)
                        let mut borrow = cell.borrow_mut();

                        // 2. 【关键】钻进 Option 内部，拿到结构体的可变引用
                        // as_mut() 会把 &mut Option<T> 变成 Option<&mut T>
                        if let Some(env) = borrow.as_mut() {
                            // 或者往 map 里插入新数据
                            // env.sessions.insert(999, new_lock);
                            Some((
                                env.sessions.clone(),
                                env.ctx.db.clone(),
                                env.ctx.connect_content.clone(),
                            ))
                        } else {
                            None
                        }
                    })
                    .unwrap();
                async move {
                    // `args` 是一个迭代器，包含了 Lua 传来的所有东西
                    // 1.【解析命令】我们从“数组”里弹出第一个元素，作为命令
                    // let cmd: String = args
                    //     .pop_front() // 弹出第一个
                    //     .ok_or_else(|| {
                    //         mlua::Error::runtime(
                    //             "redis.call requires at least one argument (the command)",
                    //         )
                    //     })?
                    //     .to_string()?; // .to_string() 自动把 LuaValue 转成 String

                    // 1. “惰性”迭代器 (计划)
                    //    类型是 Iterator<Item = Result<Frame, Error>>
                    let frame_iterator = args.into_iter().map(lua_value_to_bulk_frame);

                    // 2. “执行”计划，处理“转换失败”（“不行就中断”）
                    //    .collect() 是“制造” Vec<Frame> 的唯一方法
                    let frames_vec: Result<Vec<Frame>, mlua::Error> = frame_iterator.collect();

                    // 3. 处理中断（报错）
                    let frames: Vec<Frame> = match frames_vec {
                        Ok(f) => f, // 成功！我们拿到了 Vec<Frame>
                        Err(e) => {
                            // 失败！我们在这里“直接报错”
                            return Err(e);
                        }
                    };

                    let command = Command::try_from(Frame::Array(frames))
                        .map_err(|e| mlua::Error::runtime("redis.call 之后进行类型转换"))?;

                    if let Some(key) = command.get_key() {
                        let shard_index = MemoryCache::get_shard_index(key);
                        let sessions = sessions;
                        let mut lock = sessions.lock().await;
                        let lock = lock.get_mut(&shard_index);

                        // 执行层代码复用
                        // 修正点：
                        // 1. 去掉了闭包里多余的 `->`
                        // 2. 修正了末尾的括号数量
                        // 3. 这一行应该是作为返回值，所以去掉了 let frame = (或者是你确实需要赋值，看下文逻辑)
                        // 这里假设你是想返回 execute_command_hook 的结果：
                        let frame = execute_command_hook(&command, db_clone, content, lock)
                            .await
                            .map_err(|_| mlua::Error::runtime("lua 脚本内部命令执行失败"))
                            .unwrap();
                        Ok(frame)
                    } else {
                        // 修正点：加上了 missing 的 else
                        Err(mlua::Error::runtime("lua 脚本内部未知错误"))
                    }
                }
            },
        )
        .map_err(|e| {
            // 把 mlua 的错误转成你的 KvError
            // 这样你的系统内部就统一了
            KvError::ProtocolError(format!("Lua脚本错误: {}", e))
        })?;
    let redis_table = lua
        .create_table()
        .map_err(|_| mlua::Error::runtime("redis.call 之后进行类型转换"))
        .unwrap();
    let _ = redis_table.set("call", redis_call);
    let _ = lua.globals().set("redis", redis_table);
    Ok(lua)
}

pub async fn init_lua_pre(
    lua: &Lua,
    command: &EvalCommand,
    command_content:CommandContext,
)  {
    let mut shard_indices: Vec<usize> = command
        .keys
        .clone()
        .into_iter()
        .map(|k| MemoryCache::get_shard_index(&k))
        .collect();
    shard_indices.sort_unstable();
    shard_indices.dedup();
    let sessions = Arc::new(Mutex::new(HashMap::new()));
    let setup_env = |lua: &Lua| -> mlua::Result<()> {
        // --- 设置 KEYS ---
        let keys_table = lua.create_table()?;
        for (i, key) in command.keys.iter().enumerate() {
            // 【绝杀点】显式调用 create_string
            // 不管 key 是什么，强制变成 Lua 的 String 类型，绝无可能变成 Table
            let lua_str = lua.create_string(key.as_bytes())?;
            keys_table.set(i + 1, lua_str)?;
        }
        lua.globals().set("KEYS", keys_table)?;

        // --- 设置 ARGV ---
        let argv_table = lua.create_table()?;
        for (i, arg) in command.args.iter().enumerate() {
            // 【绝杀点】同理，强制转 String
            let lua_str = lua.create_string(arg.as_bytes())?;
            argv_table.set(i + 1, lua_str)?;
        }
        lua.globals().set("ARGV", argv_table)?;

        Ok(())
    };
    let _ = setup_env(lua)
        .map_err(|e| KvError::ProtocolError(format!("Lua全局变量(KEYS/ARGV)注入失败: {}", e)));
    //env.sessions = Some(sessions.clone());

    for shard_index in shard_indices {
        let db = command_content.db.clone().unwrap();
        // lock_shard_write 是异步的，这里会等待直到拿到锁
        // 因为是按顺序的，所以绝对不会死锁
        let guard = db.store.lock_write_lua(shard_index).await;

        // 抢到了！包装成 Session 塞进 Map
        sessions.lock().await.insert(shard_index, guard);
    }
    // 组装环境包
        let env = CurrentRequestEnv {
            ctx: command_content,
            sessions,
            command:command.clone()
        };

        // 这一步非常快，不存在阻塞
        CURRENT_ENV.with(|cell| {
            *cell.borrow_mut() = Some(env);
        });
}
//初始化放入通道
pub async fn init_lua_vm(sender: Sender<Lua>) -> (Runtime, Handle) {
    // 1. 【【【 你要的“专用池” 】】】
    let lua_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    // 2. 拿到这个“专用池”的“遥控器” (Handle)
    let lua_runtime_handle: Handle = lua_runtime.handle().clone();
    for _ in 0..50 {
        let _ = sender.send(Lua::new());
    }
    (lua_runtime, lua_runtime_handle)
}
