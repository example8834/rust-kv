use std::sync::{atomic::AtomicUsize, Arc};

use once_cell::sync::Lazy;

pub struct Config {
    pub eviction_type: EvictionType,
}
pub enum EvictionType {
    LRU,
    LFU,
}


// 注意 `pub` 关键字，这样其他模块才能访问它
pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    println!("--- Loading configuration ---");
    Config {
        eviction_type: EvictionType::LRU, // 这里可以根据需要加载不同的配置
    }
});

