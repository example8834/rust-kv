use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    f32::consts::E,
    sync::{Arc, atomic::Ordering},
    thread::JoinHandle,
    time::Duration,
};

use rand::Rng;
use serde::de::value;
use tokio::{sync::Mutex, time::Instant};

const EVICTION_MAX_NUMBER: usize = 5;

use crate::{
    core_time::get_cached_time_ms,
    db::{Storage, eviction::lru::lru_struct::LruMemoryCache},
    shutdown,
};

impl Storage {
    /**
     * 淘汰逻辑
     */
    pub async fn eviction_ttl(self, shutdown_tx: tokio::sync::broadcast::Sender<()>) {
        let mut receiver = shutdown_tx.subscribe();
        loop {
            //如果通知来了 就会下次循环会直接break
            tokio::select! {
                _ = receiver.recv() =>{
                    break;
                },
                _ = tokio::time::sleep(Duration::from_millis(100)) =>{

                }
            }
            //先创建一个数组存储
            let mut active_shards: Vec<(usize, usize)> = Vec::new();
            for db_index in 0..16 {
                for shard_index in 0..32 {
                    let shard = self.store[db_index].message[shard_index].read().await;
                    if shard.approx_memory.load(Ordering::Relaxed) > 0 {
                        active_shards.push((db_index, shard_index));
                    }
                }
            }
            if active_shards.is_empty() {
                continue;
            }

            let mut keys_check = 20;

            while keys_check > 0 && !active_shards.is_empty() {
                // 5. 从“活跃分片列表”中随机挑一个
                let random_active_index = rand::thread_rng().gen_range(0..active_shards.len());
                let (db_index, shard_index) = active_shards[random_active_index];
                //抽取后获取锁
                let shard = self.store[db_index].message[shard_index].write().await;
                //获取锁后再次判断 如果没有数据就跳过了
                if shard.approx_memory.load(Ordering::Relaxed) == 0 {
                    //说明这个分片已经没有数据了
                    active_shards.swap_remove(random_active_index);
                    continue;
                }
                //随机从当前分片 抽取一个key
                let random_active_index = rand::thread_rng().gen_range(0..shard.sample_keys.len());
                //获取key
                let key = shard.sample_keys.get(random_active_index).cloned().unwrap();
                if let Some(value) = shard.db_store.get(&key) {
                    if let Some(expire_time) = value.expires_at {
                        if get_cached_time_ms() > expire_time {
                            //更新分片和整体内存数据
                            let data_size = value.data_size;
                            shard.approx_memory.fetch_sub(data_size, Ordering::Relaxed);
                            //调用方法删除
                            let _ = LruMemoryCache::pop(shard, key).await;
                        }
                    }
                }
                keys_check -= 1;
            }
        }
    }

    //这个方法有点长 由于这个是内存管理专门的方法 不用复用了 直接写在一起了
    pub async fn eviction_memory(
        self,
        target_memory: usize,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
    ) -> Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> {
        let shutdown_clone = shutdown_tx.clone();
        let task_vec: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Vec::new().into());
        let task_vec_clone = task_vec.clone();
        //定时任务接收者
        loop {
            let mut shutdown = shutdown_clone.clone().subscribe();
            //如果通知来了 就会下次循环会直接break
            tokio::select! {
                _ = shutdown.recv() =>{
                    break;
                },
                _ = tokio::time::sleep(Duration::from_millis(100)) =>{

                }
            }
            let store = self.store.clone();
            if Storage::get_global_memory(store.clone()).await > target_memory {
                //根据指标挑选前五的分片
                let mut shard_indices: BinaryHeap<Reverse<(usize, usize, usize)>> =
                    BinaryHeap::with_capacity(EVICTION_MAX_NUMBER);
                for db_index in 0..16 {
                    for shard_index in 0..32 {
                        let store = self.store.clone();
                        let shard = store[db_index].message[shard_index].read().await;
                        let memory = shard.approx_memory.load(Ordering::Relaxed);
                        //跳过为空的
                        if memory == 0 {
                            continue;
                        }
                        // 2. 创建元组“值”
                        let tuple_value = (memory, db_index, shard_index);
                        // 3. 使用圆括号 () 把“值”包装起来
                        //    这创建了一个 Reverse<(usize, usize, usize)> 类型的 *值*
                        let item_for_heap = Reverse(tuple_value);
                        shard_indices.push(item_for_heap);
                    }
                }
                for item in shard_indices {
                    //每次循环都需要克隆
                    let shutdown_clone = shutdown_tx.clone();
                    let (_, db_index, shard_index) = item.0;
                    let store = store.clone();
                    //先获取锁 然后执行指定的时间段
                    let shard = store[db_index].message[shard_index].clone();
                    // 内存超了，开一个任务
                    let task_delete = tokio::spawn(async move {
                        let mut shard_lock = shard.write().await;
                        let mut processed_count = 0;
                        //设置开始时间
                        let start_stopwatch = Instant::now();
                        let time_budget = Duration::from_millis(10);
                        loop {
                            let mut shutdown = shutdown_clone.clone().subscribe();
                            //如果通知来了 就会下次循环会直接break
                            tokio::select! {
                                _ = shutdown.recv() =>{
                                    break;
                                },
                                _ = tokio::time::sleep(Duration::from_millis(100)) =>{

                                }
                            }

                            //再次精确判断 锁内部判断就完全没有问题了
                            if Storage::get_global_memory(store.clone()).await > target_memory {
                                if let Some(key) = shard_lock.list.pop_front() {
                                    let data_size =
                                        shard_lock.db_store.get(&key).unwrap().data_size;
                                    //从map里获取出来各种索引 对于数据结构进行修改
                                    if let Some(node_ptr) = shard_lock.map_key.remove(&key) {
                                        //  从 Vec 中删除 (O(1))
                                        let idx_to_remove = node_ptr.sample_idx;
                                        shard_lock.sample_keys.swap_remove(idx_to_remove);

                                        let moved_key_cloned =
                                            shard_lock.sample_keys.get(idx_to_remove).cloned();

                                        // 现在 `moved_key_cloned` 是一个拥有的值，它不借用 shard
                                        if let Some(key) = moved_key_cloned {
                                            // 6. 我们可以【安全地】对 shard 进行【可变借用】
                                            if let Some(moved_meta) =
                                                shard_lock.map_key.get_mut(&key)
                                            {
                                                moved_meta.sample_idx = idx_to_remove;
                                            }
                                        }
                                        //现在删除内存更新分片和全局内存数据
                                        shard_lock
                                            .approx_memory
                                            .fetch_sub(data_size, Ordering::Relaxed);
                                        processed_count += 1;
                                    }
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                            //精准判断时间
                            if processed_count % 10 == 0 {
                                if start_stopwatch.elapsed() > time_budget {
                                    break;
                                }
                            }
                        }
                    });
                    task_vec_clone.lock().await.push(task_delete);
                }
            }
        }
        task_vec
    }
    //这个异步方法确实不错
    //通过计算获取全局数据总和
    pub async fn get_global_memory(store :Arc<Vec<Arc<LruMemoryCache>>>) ->usize {
        let mut global_memory  = 0;
        //这个是通过计算每个分片获取数据
            for db_index in 0..16 {
                
                for shard_index in 0..32 {
                    let shard = store[db_index].message[shard_index].read().await;
                    let shard_memory = shard.approx_memory.load(Ordering::Relaxed);
                    global_memory += shard.approx_memory.load(Ordering::Relaxed);
                }
            }
        global_memory
    }
}
