pub mod lru_linklist;
pub mod lru_struct;
#[derive(Default, Debug, Clone)]
pub struct LruCache;


pub struct LruEntry {
    pub hash_vec :Vec<LruShare>,
}

pub struct LruShare{
   
}