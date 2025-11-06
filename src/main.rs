#[tokio::main] // 这个宏将 main 函数标记为 Tokio 运行时入口点
async fn main() {
    kv::run().await
}
// 1. 这是你的“正常”代码
pub fn add(left: usize, right: usize) -> usize {
    left + right
}




// 你的“正常”代码在上面...
// ...

#[cfg(test)]
mod tests {
    // 引入你“上面”写的代码
    use super::*; 
    // 引入 mlua
    use mlua::prelude::*;

    // 注意：我们用的是 `#[tokio::test]`，而不是 `#[test]`
    // 因为我们的函数是 `async` 的
    #[tokio::test]
    async fn test_lua_hello_world() {
        // 1. 创建 Lua 虚拟机
        let lua = Lua::new();

        // 2. 异步执行脚本
        //    (我们用 .await 替代了 .unwrap() 来处理异步)
        let result: i32 = lua.load("return 1.123 + 1231")
            .eval_async()
            .await
            .expect("Lua 脚本执行失败!"); // 如果失败，测试会在这里 panic

        // 3. 断言结果
        println!("{}", result);

        println!("--- Lua 脚本测试通过！ 1 + 1 = 2 ---");
    }
}