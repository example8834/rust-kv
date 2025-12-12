# Rust-KV: 高性能企业级异步内存数据库

**Rust-KV** 是一个从零构建的、兼容 Redis 协议 (RESP) 的高性能分布式内存数据库原型。

本项目不仅是一个 KV 存储，更是一个深度探索 **Rust 异步运行时 (Tokio)**、**无锁编程**、**Actor 模型** 以及 **FFI 高性能交互** 的工业级实践。它在单机环境下实现了超高并发与极低延迟，特别是在 Lua 脚本执行引擎上，通过创新的架构设计，实现了超越原生 Redis 的 ACID 保证。

## 🚀 极致性能 (Performance Benchmark)

基于 Mac本地回环测试，使用工业级压测工具 `memtier_benchmark` (开启多线程 + Pipeline) 测得：

| 测试场景 (Benchmark Scenario) | QPS (Requests/sec) | 核心优势与瓶颈分析 |
| :--- | :--- | :--- |
| **混合读写 (Mixed SET/GET)** | **1,000,000+** | **百万级吞吐！** 4线程并发彻底释放多核性能，Actor 模型无锁调度成为关键 |
| **纯内存读取 (GET)** | **630,000+** | **Sharded SwissTable** 结合零拷贝解析 (Zero-Copy)，极致利用 CPU L1/L2 缓存 |
| **纯内存写入 (SET)** | **450,000+** | **多队列分片** 配合 Group Commit 策略，在高并发写入下完美消除锁竞争 |

> **复现指令参考:**
>
> ```bash
> memtier_benchmark -s 127.0.0.1 -p 6379 -t 4 -c 50 --pipeline=32 --ratio=1:1
> ```

## 🛠️ 核心架构与技术亮点 (Core Architecture)

### 1. 革命性的多线程 Lua 引擎 (Multi-Reactor Lua Engine)
这是本项目最核心的创新点，解决了 Redis 单线程脚本阻塞的痛点，同时保证了比 Redis 更强的数据一致性。

* **Actor 模型架构:** 启动 N 个（默认 8 个）独立的 OS 线程，每个线程独占一个 Lua 虚拟机 (`mlua::Lua`)。外部请求通过 `flume/mpsc` 通道分发，实现了计算与 I/O 的彻底分离。
* **ThreadLocal 零开销注入 (Zero-Overhead Injection):**
    * 拒绝每次请求重复创建 Lua 上下文。
    * 利用 Rust 的 `thread_local!`，在线程启动时一次性注册 `redis.call` 绑定。
    * 运行时仅通过 TLS 指针交换上下文 (`CURRENT_ENV`)，将 FFI 调用开销降至纳秒级，从而实现了 **11w+ QPS** 的惊人性能。
* **智能负载均衡 (Queue-Aware Dispatch):** 调度器实时监控每个 Worker 的队列深度 (`capacity`)，自动将任务分发给最空闲的线程，避免了 Round-Robin 导致的队头阻塞 (Head-of-Line Blocking) 问题。

### 2. 真·ACID 事务支持 (True ACID Transactions)
超越 Redis 的 "Scripting" 语义，实现了真正的数据库级事务。

* **写缓冲 (Write Buffering):** Lua 脚本执行期间，所有的写入操作 (`SET`, `DEL`) 不会直接修改底层数据，而是记录在 `LuaCacheNode` 的 `differ_map` 差异缓冲区中。
* **原子提交与回滚 (Commit or Rollback):**
    * **Success:** 只有脚本成功返回，差异数据才会原子性地应用到底层存储。
    * **Failure:** 如果脚本中途报错（Panic 或 Error），缓冲区直接丢弃，底层数据毫发无损。
* **对比:** 原生 Redis 脚本若中途失败，已执行的写操作无法撤销，破坏原子性。

### 3. 精细化分片锁架构 (Sharded Locking)
为了在多线程环境下最大化并发度，彻底摒弃全局大锁。

* **两级分片:** `16 Databases` × `64 Shards/DB` = **1024 个独立的锁域**。
* **无锁哈希:** 采用 `fxhash` 进行极速路由。
* **锁粒度控制:** 读写操作仅锁定 Key 所在的特定分片 (`RwLock`)，使得 99% 的并发请求完全无竞争。

### 4. 智能 AOF 持久化 (Smart Batching AOF)
解决了高并发写入下的磁盘 I/O 瓶颈。

* **背压与削峰:** AOF 通道 (`mpsc::channel`) 作为天然的缓冲区，吸收突发流量。
* **贪婪批处理 (Greedy Batching):** 后台落盘任务 (`aof_writer_task`) 采用“贪婪模式”：一旦唤醒，会尽可能多地从通道中拉取积压数据（比如一次 5000 条），合并为一次 `write_all` 和 `flush` 系统调用。这使得系统在 IOPS 有限的 SSD 上也能跑满带宽。

### 5. 健壮的工程化实现
* **优雅停机 (Graceful Shutdown):** 基于 `broadcast` 通道实现的双层停机（应用层 -> 基础设施层），确保在服务关闭前，所有挂起的 AOF 数据都被刷入磁盘，数据零丢失。
* **Unsafe 手写 LRU:** 为了追求极致的 `O(1)` 淘汰性能，使用 `NonNull` 裸指针手写双向链表，结合 `HashMap` 索引，实现了生产级的 LRU 淘汰算法。
* **零拷贝协议解析:** 基于 `bytes::BytesMut` 和 `Cursor` 实现的 RESP 解析器，在解析过程中零内存分配。
