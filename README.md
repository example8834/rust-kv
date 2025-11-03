# 高性能异步 Rust KV 数据库 (kv-rs)

这是一个使用 Rust 和 [Tokio](https://tokio.rs/) 从零开始构建的高性能、异步、内存键值数据库原型。

本项目深度集成了现代并发编程模式和系统设计，专注于实现一个高并发、低延迟、高健壮性的内存数据库服务。它不仅是一个功能实现，更是一个对生产级服务器架构的深度实践。

## 核心技术亮点 (Technical Highlights)

本项目的“技术含金量”体现在对并发、锁、内存管理和系统健壮性的精细化处理上：

### 1\. 生产级服务编排与优雅停机

项目实现了一套完整的、健壮的服务生命周期管理和优雅停机（Graceful Shutdown）机制。

  * **双通道停机广播:** 使用两个 `tokio::sync::broadcast` 通道（`app_shutdown_tx` 和 `infra_shutdown_tx`）来实现分阶段停机。
  * **黄金停机顺序:** 严格遵循“先停应用、再停地基”的原则。`Ctrl+C` (`SIGINT`) 或 `SIGTERM` 信号会触发 `app_shutdown_tx`，使服务器立即停止接受新连接 (`listener.accept()`) 和新任务 (`aof_writer_task` 等)。
  * **JoinHandle 编排:** `main` 函数会 `await` 所有核心任务（如连接管理、内存淘汰、AOF 任务）的 `JoinHandle`，确保它们全部执行完毕。
  * **最后关闭地基:** 直到所有应用逻辑（包括AOF刷盘）都安全结束后，才会发送 `infra_shutdown_tx` 信号，关闭最底层的服务（如全局时间缓存任务）。

### 2\. 精细化分片锁 (Sharded Lock) 架构

为了实现极高的并发性能，数据库的锁粒度被设计得非常精细，彻底避免了“全局大锁”。

  * **DB 实例分片:** `Storage` 结构持有一个包含16个 `Arc<LruMemoryCache>` 的 `Vec`，模拟16个独立的数据库实例。
  * **内部哈希分片:** 每个 `LruMemoryCache` 内部**再次**被分为32个分片 (`NUM_SHARDS: usize = 32`)，每个分片拥有一个独立的 `RwLock`。
  * **高性能哈希:** 使用 `fxhash`（一种非加密的快速哈希算法）来决定 Key 落在哪个内部哈G片上，进一步减少锁竞争。

### 3\. 高级内存淘汰 (Eviction) 策略

项目实现了 Redis 同款的、复杂的近似内存淘汰算法，兼顾了性能和效率。

  * **近似 TTL 淘汰:** 后台任务 (`eviction_ttl`) 采用**随机采样**策略，而非遍历所有 Key。它随机挑选活跃的分片和 Key，检查其 TTL 是否过期，这是一种高效的近似算法。
  * **真·LRU 算法:** 为实现高性能 LRU，使用 `unsafe` Rust 手动实现了一个基于裸指针 (`NonNull<Node>`) 的双向链表，用于 `O(1)` 时间复杂度的节点移动。
  * **并发与时间预算淘汰 (最亮眼的设计):**
    1.  当全局内存 (`GLOBAL_MEMORY`) 超出限制时。
    2.  `eviction_memory` 任务使用**小顶堆** (`BinaryHeap`) 快速找出内存占用 **Top N** 的分片。
    3.  **并发执行:** 为每个 Top N 分片**并发地 `tokio::spawn` 一个专属的清理任务**。
    4.  **时间预算:** 每个清理任务只持有分片锁 10 毫秒（`time_budget`），清理固定数量的 Key 后就**主动释放锁**并重新 `await`，避免长时间阻塞正常的读写请求。

### 4\. 健壮的网络层与 AOF 持久化

  * **健壮的 RESP 解析器:** `parse_frame` 函数使用 `Cursor` 在只读的缓冲区切片上进行“预演”解析。只有在“预演”成功（即收到了一个完整的 Frame）后，才会推进 (`advance`) `BytesMut` 缓冲区，完美地处理了 TCP 粘包和半包问题。
  * **批量 AOF 写入:** AOF 任务 (`aof_writer_task`) 不会每来一条命令就写一次磁盘。它使用 `tokio::select!` 配合 `interval.tick()` 和 `rx.try_recv()`，每秒钟或在收到关闭信号时，从 `mpsc` 通道中**批量拉取**所有待处理的命令，进行一次性的 `write_all` 和 `flush`，大幅提高了I/O效率。

### 5\. 性能与上下文微优化

  * **全局原子时间缓存:** 使用 `static CACHED_TIME_MS: AtomicU64`，由一个10ms更新一次的 `infra` 任务（`start_time_caching_task`）维护。所有业务逻辑（如TTL检查、AOF转换）都从这个原子变量中读取时间，避免了频繁的 `SystemTime::now()` 系统调用。
  * **`task_local!` 上下文管理:** 使用 `tokio::task_local! { pub static CONN_STATE: ... }` 来存储每个连接的上下文（如当前选择的DB）。这使得 `db.lock_write` 等深层函数可以“透明”地获取到当前连接的状态，而不需要在每个函数签名中“钻孔”式地传递 `db_index`。

## 如何运行

```bash
# 克隆项目
git clone ...

# 运行服务器
cargo run
```

```bash
# 在另一个终端使用 redis-cli 连接
redis-cli
```
