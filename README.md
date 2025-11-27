# Rust-KV: 高性能异步 Redis 协议兼容数据库
[](https://www.rust-lang.org/)
[](https://www.google.com/search?q=LICENSE)

Rust-KV 是一个从零构建的、兼容 RESP 协议的高性能内存键值数据库。
本项目不仅是一个简单的 KV 存储，更是一个探索 Rust **异步运行时架构**、**Actor 模型**以及**无锁编程**的深度实践项目。

> **核心特性概览：** 完整的 Lua 脚本支持 (Async VM)、精细化分片锁架构、手写 Unsafe LRU 淘汰、AOF 持久化、优雅停机。

## 🚀 核心技术架构 (Architecture Highlights)

本项目历时 3 个月迭代，重构了核心内核以支持更复杂的业务场景，主要包含以下技术亮点：

### 1\. 异步 Lua 脚本引擎 (Async Lua Architecture)

为了在多线程 Tokio 运行时中高效支持 Lua 脚本（Lua VM 本身是 `!Send` 的），本项目设计了一套基于 **Actor 模型** 的脚本执行引擎：

  * **独立运行时**：启动一个专用的 OS 线程，并在内部运行 `tokio::task::LocalSet`，专门用于承载 Lua 虚拟机。
  * **Actor 通信**：外部请求通过 `flume` 通道发送 `LuaTask` 消息，包含执行上下文和 `oneshot` 回调通道。
  * **原生互操作**：实现了 `redis.call` 接口，允许 Lua 脚本异步回调 Rust 原生的 `CommandExecutor`，复用底层的锁逻辑和指令处理逻辑，实现了脚本层与原生层的无缝融合。

### 2\. 高并发分片存储 (Sharded Storage)

为了避免全局锁竞争（Global Lock Contention），存储层采用了两级分片架构：

  * **物理库分片**：模拟 Redis 的 16 个独立数据库 (`db_index`)。
  * **内部哈希分片**：每个数据库内部被进一步划分为 **32 个分片 (Shards)**。
  * **锁粒度控制**：Key 通过 `FxHash` 映射到特定分片，读写操作仅锁定对应的 `RwLock`，极大提升了并发吞吐量。

### 3\. 高性能内存管理 (Memory & Eviction)

  * **Unsafe LRU**：为了实现极致的 O(1) 性能，抛弃了标准库容器，使用 `NonNull` 裸指针手写了双向链表来实现 LRU 缓存淘汰策略。
  * **近似淘汰算法**：实现了类似 Redis 的随机采样淘汰机制 (`eviction_ttl`) 和基于小顶堆 (`BinaryHeap`) 的全局内存监控任务 (`eviction_memory`)。
  * **原子化记账**：使用 `AtomicUsize` 进行内存使用的实时统计，对性能几乎无损。

### 4\. 健壮的工程实现

  * **RESP 协议解析**：基于 `bytes::BytesMut` 和 `Cursor` 实现了零拷贝（Zero-Copy）的高效 RESP 协议解析器，完美处理 TCP 粘包/半包问题。
  * **优雅停机 (Graceful Shutdown)**：基于广播通道 (`broadcast::channel`) 编排了复杂的停机顺序（应用层 -\> 任务层 -\> 基础设施层），确保 AOF 刷盘完成且所有连接处理完毕后才关闭服务。
  * **AOF 持久化**：实现了批量写屏障，利用 Channel 缓冲突发流量，定期批量 `flush` 磁盘。

## 🛠️ 项目结构 (Project Structure)

```text
src/
├── lua/                # [NEW] Lua 脚本引擎核心
│   ├── lua_vm.rs       # Lua 虚拟机逻辑 & redis.call 实现
│   ├── lua_work.rs     # 基于 LocalSet 的 Actor 线程模型
│   └── lua_exchange.rs # Lua Value <-> Redis Frame 类型转换
├── db/
│   ├── mod.rs          # 核心存储结构 Db/Storage
│   └── eviction/       # 淘汰策略 (包含手写的 Unsafe LRU)
├── command_execute/    # [Refactor] 指令执行层，支持动态分发
├── core_exchange.rs    # 协议转换层
├── server.rs           # Tokio TCP 连接处理循环
└── main.rs             # 应用程序入口与服务编排
```

## ⚡ 快速开始 (Getting Started)

### 环境要求

  * Rust (Latest Stable)
  * Redis-cli (可选，用于测试)

### 运行服务器

```bash
# 克隆项目
git clone https://github.com/your-repo/rust-kv.git

# 运行 (Release 模式性能更佳)
cargo run --release
```

服务器默认监听 `127.0.0.1:6379`。

### 测试 Lua 脚本

你可以使用标准的 `redis-cli` 进行交互：

```bash
$ redis-cli

# 设置值
127.0.0.1:6379> SET mykey "Hello Rust"
OK

# 执行 Lua 脚本 (在脚本中调用 Rust 原生指令)
127.0.0.1:6379> EVAL "return redis.call('GET', KEYS[1])" 1 mykey
"Hello Rust"

# 复杂的原子操作演示
127.0.0.1:6379> EVAL "local v = redis.call('GET', KEYS[1]); return v .. ' World'" 1 mykey
"Hello Rust World"
```

## 🔮 未来规划 (Roadmap) 目前架构处于搭建状态 之后具体内容都开始填充
  * [x] 基础 KV 命令 (SET/GET/PING/EXPIRE)
  * [x] 核心存储架构 (分片锁)
  * [x] AOF 持久化
  * [x] **Lua 脚本支持 (EVAL)**
  * [ ] RDB 快照支持
  * [ ] Cluster 集群模式
  * [ ] Stream 数据结构支持

## 📄 License

MIT License

