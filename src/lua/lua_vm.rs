use flume::{Receiver, Sender};
use mlua::{Error, Lua};
use tokio::runtime::{Handle, Runtime};

use crate::{
    command_exchange::extract_bulk_string,
    db::Db,
    error::{Command, Frame, KvError},
    lua::lua_exchange::lua_value_to_bulk_frame,
};

/*
处理lua 的vm 的脚本执行方法
*/
pub async fn lua_vm_redis_call(receivce: Receiver<Lua>, db: Db, lua_handle: Handle) {
    let lua = receivce.recv_async().await.unwrap();
    // 3.【核心】这就是你说的“回调结构”！
    //    我们正在创建一个 Lua 能调用的 Rust 异步函数
    let redis_call: mlua::Function<'_> = lua.create_async_function(
        // 关键改变在这里！我们只接收一个 `args`，它包含了所有参数！
        move |lua_ctx, mut args: mlua::MultiValue| async move {
            // `args` 是一个迭代器，包含了 Lua 传来的所有东西

            // 1.【解析命令】我们从“数组”里弹出第一个元素，作为命令
            let cmd: String = args
                .pop_front() // 弹出第一个
                .ok_or_else(|| {
                    mlua::Error::runtime("redis.call requires at least one argument (the command)")
                })?
                .to_string()?; // .to_string() 自动把 LuaValue 转成 String

            // 2.【解析参数】现在 `args` 里剩下的所有东西，都是命令的参数
            //    (比如 'my_key', 'val1', 'val2', ...)

            let a: Option<&mlua::Value<'_>> = args.iter().next();
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

            //调整参数传入
            let command = Command::try_from(Frame::Array(frames))
                .map_err(|e| mlua::Error::runtime("redis.call 之后进行类型转换"))?;
            ()
        },
    )?; // 'async_function' 是因为我们的 get/set 是 async 的

    
    let redis_table = lua
        .create_table()
        .map_err(|e| mlua::Error::runtime("redis.call 之后进行类型转换"))?;

    redis_table.set("call", redis_call);

    lua.globals().set("redis", redis_table);

    lua_handle.spawn(async move {
        let result = lua.load(script).eval_async().await;
    });


}

//初始化放入通道
pub async fn init_lua_vm(sender: Sender<Lua>) -> (Runtime, Handle) {
    // 1. 【【【 你要的“专用池” 】】】
    let lua_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2) // 👈 “制定占用俩核心”
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
