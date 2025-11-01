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


// 它是一个“懒加载”的、线程安全的、全局唯一的 Arc<AtomicUsize>
pub static GLOBAL_MEMORY: Lazy<Arc<AtomicUsize>> = Lazy::new(|| {
    // 这里的代码只会在程序第一次访问 GLOBAL_MEMORY 时执行一次
    Arc::new(AtomicUsize::new(0))
});