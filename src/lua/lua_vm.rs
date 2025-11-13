use flume::{Receiver, Sender};
use mlua::Lua;
use tokio::runtime::{Handle, Runtime};

/*
 处理lua 的vm 的脚本执行方法
 */
pub async fn lua_vm_redis_call(receivce : Receiver<Lua>){
    let lua = receivce.recv_async().await.unwrap();
    //lua.reset();
    //lua.set_app_data(data)
}

//初始化放入通道
pub async fn init_lua_vm(sender: Sender<Lua>) -> (Runtime,Handle){
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
    (lua_runtime,lua_runtime_handle)
}