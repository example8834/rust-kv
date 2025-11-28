use std::{collections::HashMap, ptr::NonNull, sync::Arc};

use rand::Rng;

use crate::db::eviction::{
    EvictionPolicy,
    lru::lru_linklist::{LruList, Node},
};

pub const NUM_SHARDS: usize = 32; // 32 个分片

pub struct LruNode {
    pub list: LruList,
    pub sample_keys: Vec<Arc<String>>, // O(1) 采样数组
    pub map_key: HashMap<Arc<String>, MetaPointers>,
}

unsafe impl Send for LruNode {}
unsafe impl Sync for LruNode {}

#[derive(Debug, Clone)]
// 你的新 Value 结构
pub struct MetaPointers {
    pub lru_node: NonNull<Node>, // 指向 LRU 链表节点
    pub sample_idx: usize,       // 指向 Vec<Key> 的索引
}

impl LruNode {
    pub fn new() -> Self {
        LruNode {
            list: LruList::new(),
            sample_keys: Vec::new(),
            map_key: HashMap::new(),
        }
    }
}

impl EvictionPolicy for LruNode {
    //通过 专门的hash算法 获取在那个分片
    fn on_write(&mut self, key: Arc<String>) {
        //接下来就是在这个分片上操作
        let contaion = self.map_key.contains_key(&key);
        if !contaion {
            let node_ptr = self.list.push_back(key.clone());

            let index = self.sample_keys.len();
            self.sample_keys.push(key.clone());

            self.map_key.insert(
                key,
                MetaPointers {
                    lru_node: node_ptr,
                    sample_idx: index,
                },
            );
        } else {
            //只能克隆 也应该克隆 这里只修改链表
            let node_ptr = self.map_key.get(&key).unwrap().clone();
            self.list.push_mid_back(node_ptr.lru_node);
        }
    }

    fn on_read(&mut self, key: &Arc<String>) {
        // 必须用 if let 或 match 来安全地处理
        if let Some(meta_ptr) = self.map_key.get(key) {
            //接下来就是在这个分片上操作
            let node_ptr = meta_ptr.lru_node;
            self.list.push_mid_back(node_ptr);
        }
    }

    fn on_delete(&mut self, key: Arc<String>) {
        // 1. 从 master_map 删除，拿到元数据
        if let Some(node_ptr) = self.map_key.remove(&key) {
            // 2. 从 LRU 链表删除
            self.list.pop_node(node_ptr.lru_node);

            // 3. 从 Vec 中删除 (O(1))
            let idx_to_remove = node_ptr.sample_idx;
            self.sample_keys.swap_remove(idx_to_remove);

            let moved_key_cloned = self.sample_keys.get(idx_to_remove).cloned();

            // 5. 现在 `moved_key_cloned` 是一个拥有的值，它不借用 shard
            if let Some(key) = moved_key_cloned {
                // 6. 我们可以【安全地】对 shard 进行【可变借用】
                if let Some(moved_meta) = self.map_key.get_mut(&key) {
                    moved_meta.sample_idx = idx_to_remove;
                }
            }
        }
    }

    fn get_random_sample_key(&self) -> Option<Arc<String>> {
        //随机从当前分片 抽取一个key
        let random_active_index = rand::thread_rng().gen_range(0..self.sample_keys.len());
        Some(self.sample_keys.get(random_active_index).cloned().unwrap())
    }

    fn pop_victim(&mut self) -> Option<Arc<String>> {
        self.list.pop_front()
    }
}
