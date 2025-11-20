// -------------------------------------------------
// 复制这里开始
// -------------------------------------------------

// 1. 这是你的“正常”代码
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

// 2. 这是你的“测试模块”（必须放在同一个文件的最底部）
#[cfg(test)]
mod tests {
    // 3. 引入上面"正常"代码里的 "add" 函数
    use super::*;

    // 4. 你的第一个测试
    #[test]
    fn it_adds_correctly() {
        assert_eq!(add(2, 2), 4);
        
        // 5. 看，这是正确的 println! 写法
        println!("--- 测试 1 跑过啦！---");
    }

    // 6. 你的第二个测试
    #[test]
    fn another_test() {
        assert!(true);
        println!("--- 测试 2 也跑过啦！---");
    }
}
// -------------------------------------------------
// 复制这里结束
// -------------------------------------------------

// --- 准备工作 ---
// 1. 请确保你的 Cargo.toml 包含了这些依赖：
// [dependencies]
// tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
// mlua = { version = "0.9", features = ["lua54", "vendored", "async"] }
// dashmap = "5" // 我们用 DashMap 来模拟你分片的、线程安全的存储

use mlua::prelude::*;
use dashmap::DashMap;
use std::sync::Arc;

// --- 模拟你的存储核心 ---
// 我们用 Arc<DashMap<...>> 来模拟你那个 Arc<Vec<Arc<...>>> 的分片存储
// 它也是线程安全的，可以被 clone
#[derive(Clone, Debug)]
struct Storage {
    db: Arc<DashMap<String, Vec<u8>>>,
}

impl Storage {
    // 创建一个新的、空的存储
    fn new() -> Self {
        Self {
            db: Arc::new(DashMap::new()),
        }
    }

    // 你的 get 方法 (我们把它改成 async 来模拟真实情况)
    async fn get(&self, key: String) -> Option<Vec<u8>> {
        // DashMap 的 get 是瞬时的，我们加个小 await 来模拟异步I/O
        // tokio::time::sleep(std::time::Duration::from_millis(1)).await; 
        self.db.get(&key).map(|v| v.value().clone())
    }

    // 你的 set 方法
    async fn set(&self, key: String, value: Vec<u8>) {
        // tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        self.db.insert(key, value);
    }
}

// --- 核心：“回调结构” 和 “连接器” ---

/// 这个函数就是“阶段 2”的实现
/// 它接收“存储”和“脚本”，然后执行它
async fn run_lua_script(storage: Storage, script: &str) -> Result<mlua::Value, mlua::Error> {
    
    // 1. 为本次执行，创建一个全新的、干净的 Lua 沙箱
    let lua = Lua::new();

    // 2.【关键】克隆你的 Storage (因为是 Arc，所以很便宜)
    //    我们准备把它 move 进 Lua 的“回调函数”里 
    //    我们叫它 `storage_for_callback` 
    let storage_for_callback = storage.clone();

    // 3.【核心】这就是你说的“回调结构”！
    //    我们正在创建一个 Lua 能调用的 Rust 异步函数
    let redis_call = lua.create_async_function(
        // `move` 关键字会把 `storage_for_callback` 的所有权“移入”这个闭包
        move |lua_ctx, (cmd, key, value): (String, String, Option<Vec<u8>>)| {
            
            // 4. 再次克隆，因为 Lua 脚本可能在一个循环里多次调用
            //    `redis.call`，所以这个闭包需要能被重复执行。
            let storage = storage_for_callback.clone(); 

            async move {
                // 5.【连接】“回调结构”在这里“连接”你写的那些方法！
                //    我们匹配 Lua 传来的命令字符串
                match cmd.to_lowercase().as_str() {
                    "get" => {
                        // 调用你真正的 Rust 异步方法
                        let result = storage.get(key).await;
                        
                        // 把 Rust 的 Option<Vec<u8>> 转换成 Lua 能理解的值
                        // Some(data) -> "data" (string)
                        // None       -> nil
                        Ok(lua_ctx.to_value(&result)?)
                    },
                    "set" => {
                        if let Some(val) = value {
                            // 调用你真正的 Rust 异步方法
                            storage.set(key, val).await;
                            
                            // 告诉 Lua "OK"
                            Ok(lua_ctx.to_value("OK")?)
                        } else {
                            // 如果 Lua 调用错了 (例如 redis.call('set', 'key'))
                            // 我们就返回一个错误
                            Err(mlua::Error::runtime("SET requires a value argument"))
                        }
                    },
                    _ => {
                        // 如果 Lua 调了一个我们不认识的命令
                        Err(mlua::Error::runtime(format!("Unknown command: {}", cmd)))
                    }
                }
            }
        })?; // 'async_function' 是因为我们的 get/set 是 async 的

    // 6.【安装】把这个“回调函数”安装到 Lua 的“全局变量”里
    //    我们要创建一张叫 `redis` 的表 (table)
    let redis_table = lua.create_table()?;
    
    // 7. 把我们刚创建的 `redis_call` 函数，放进这张表里，并命名为 `call`
    redis_table.set("call", redis_call)?;
    
    // 8. 把这张 `redis` 表，设置为 Lua 的“全局变量”
    lua.globals().set("redis", redis_table)?;

    // 9.【执行】一切就绪！执行用户传来的脚本
    //    `eval_async` 会在遇到 `redis.call` 时自动暂停，
    //    等待我们的 Rust 回调 (future) 执行完毕，然后再继续
    let result = lua.load(script).eval_async().await?;

    // 10. 返回 Lua 脚本的最终执行结果
    Ok(result)
}


// --- 运行这个例子 ---
#[tokio::main]
async fn main() {
    // 1. 创建我们的数据库实例
    let my_db = Storage::new();

    // 2. 我们先手动往里面放一个值，假装数据库里本来就有
    my_db.set("hello".to_string(), b"world".to_vec()).await;
    println!("Rust: 数据库 'hello' 初始值 = {:?}", my_db.get("hello".to_string()).await);
    
    // 3. 这是用户（客户端）发来的 Lua 脚本
    //    这个脚本的逻辑是：
    //    a. 拿一下 'hello' 的值
    //    b. 把 'new_key' 设置成 'lua_was_here'
    //    c. 返回 'hello' 的旧值
    let script = r#"
        local old_val = redis.call('GET', 'hello')
        redis.call('SET', 'new_key', 'lua_was_here')
        return old_val
    "#;

    // 4. 执行脚本！
    println!("\nRust: --- 开始执行 Lua 脚本 ---");
    match run_lua_script(my_db.clone(), script).await {
        Ok(lua_result) => {
            // 我们拿到了 Lua 脚本 `return` 的值
            let returned_value: Option<String> = FromLua::from_lua(lua_result, &Lua::new()).unwrap();
            println!("Rust: Lua 脚本返回了 = {:?}", returned_value);
        }
        Err(e) => {
            println!("Rust: Lua 脚本执行失败 = {}", e);
        }
    }
    println!("Rust: --- Lua 脚本执行完毕 ---\n");


    // 5. 检查数据库的最终状态
    println!("Rust: 'hello' 的值 (没变) = {:?}", my_db.get("hello".to_string()).await);
    println!("Rust: 'new_key' 的值 (被 Lua 设置了) = {:?}", my_db.get("new_key".to_string()).await);
}