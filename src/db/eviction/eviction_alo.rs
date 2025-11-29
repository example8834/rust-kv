use std::{
    cmp::Reverse, collections::BinaryHeap, sync::Arc, time::Duration, u32, usize
};

use rand::Rng;
use tokio::{sync::Mutex, time::Instant};

const EVICTION_MAX_NUMBER: usize = 5;

use crate::{
    core_time::get_cached_time_ms,
    db::{Storage, eviction::{LockOwner, MemoryCache, NUM_SHARDS}},
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
                for shard_index in 0..NUM_SHARDS {
                    let shard = self.get_lock_read(db_index, shard_index).await;
                    if shard.get_memory_usage() > 0 {
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
                let mut shard = self.get_lock_write(db_index, shard_index).await;
                //获取锁后再次判断 如果没有数据就跳过了
                if shard.get_memory_usage() == 0 {
                    //说明这个分片已经没有数据了
                    active_shards.swap_remove(random_active_index);
                    continue;
                }
                let key = shard.get_eviction_policy().await.unwrap().get_random_sample_key().unwrap();
                if let Some(value) = shard.select(&key).await {
                    if let Some(expire_time) = value.expires_at {
                        if get_cached_time_ms() > expire_time {
                            //更新分片和整体内存数据
                            let data_size = value.data_size;
                            shard.add_memory(data_size);
                            //调用方法删除
                            let _ = shard.get_eviction_policy().await.unwrap().on_delete(key);
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
            if Storage::get_global_memory_can_move(&self,target_memory).await {
                //根据指标挑选前五的分片
                let mut shard_indices: BinaryHeap<Reverse<(usize, usize, usize)>> =
                    BinaryHeap::with_capacity(EVICTION_MAX_NUMBER);
                for db_index in 0..16 {
                    for shard_index in 0..NUM_SHARDS {
                        let shard =  self.get_lock_read(db_index, shard_index).await;
                        let memory = shard.get_memory_usage();
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
                    //先获取锁 然后执行指定的时间段
                    let mut shard_lock = self.get_lock_read(db_index, shard_index).await;
                    let store: Arc<Vec<Arc<MemoryCache>>> = self.store.clone();
                    // 内存超了，开一个任务
                    let task_delete = tokio::spawn(async move {
                        //let mut shard_lock = shard.await;
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
                            if Storage::get_global_memory_not_move(store.clone(),target_memory).await {
                                let key = shard_lock.get_eviction_policy().await.unwrap().pop_victim();
                                if let Some(key) = key {
                                    let data_size =
                                        shard_lock.select(&key).await.unwrap().data_size;
                                    shard_lock.get_eviction_policy().await.unwrap().on_delete(key);
                                    //现在删除内存更新分片和全局内存数据
                                    shard_lock.sub_memory(data_size);
                                    processed_count += 1;
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
    //锁资源一定要精确计算获取
    pub async fn get_global_memory_can_move(&self,max_size:usize) -> bool {
        let mut global_memory = 0;
        //这个是通过计算每个分片获取数据
        for db_index in 0..16 {
            for shard_index in 0..NUM_SHARDS {
                let shard_lock = self.get_lock_read(db_index, shard_index).await;
                global_memory += shard_lock.as_lock_owner().unwrap().get_memory_usage();
                if global_memory > max_size {
                    return true;
                }
            }
        }
        return false;
    }

    //self 一定不能move 这个还是最基本的在循环里
    pub async fn get_global_memory_not_move(store: Arc<Vec<Arc<MemoryCache>>>,max_size:usize) -> bool {
        let mut global_memory = 0;
        //这个是通过计算每个分片获取数据
        for db_index in 0..16 {
            for shard_index in 0..NUM_SHARDS {
                let shard_lock = store[db_index].message[shard_index].clone().read_owned().await;
                global_memory += shard_lock.get_memory_usage();
                if global_memory > max_size {
                    return true;
                }
            }
        }
        return false;
    }
}
