use std::{ptr::NonNull, sync::Arc};

#[derive(Debug)]
pub struct Node {
    key: Arc<String>,
    prev: Option<NonNull<Node>>,
    next: Option<NonNull<Node>>,
}

impl Node {
    fn new(key: Arc<String>) -> Self {
        Node {
            key,
            prev: None,
            next: None,
        }
    }
}

#[derive(Debug, Clone )]
pub struct LruList {
    head: Option<NonNull<Node>>,
    tail: Option<NonNull<Node>>,
    len: usize,
}

// 承诺 里面unsafe的指针被管理了 肯定不会出问题
unsafe impl Send for LruList {}
unsafe impl Sync for LruList {}

impl LruList {
    pub fn new() -> Self {
        LruList {
            head: None,
            tail: None,
            len: 0,
        }
    }
    //这个是直接添加到末尾
    pub fn push_back(&mut self, key: Arc<String>) -> NonNull<Node> {
        let new_node = Node::new(key);
        self.len += 1;
        let new_node_ptr = NonNull::new(Box::into_raw(Box::new(new_node))).unwrap();
        //如果头节点为空，说明链表为空
        if self.head.is_none() {
            self.head = Some(new_node_ptr);
            self.tail = Some(new_node_ptr);
            return new_node_ptr;
        }
        //链表不是空的情况
        //这一步就是把新节点插入到尾节点后面
        if let Some(tail) = self.tail {
            unsafe {
                (*tail.as_ptr()).next = Some(new_node_ptr);
                (*new_node_ptr.as_ptr()).prev = Some(tail);
            }
        }
        //这是尾巴节点指向新节点
        self.tail = Some(new_node_ptr);
        new_node_ptr
    }


    pub fn push_mid_back(&mut self, node_ptr: NonNull<Node>) {
        // 如果节点已经是尾节点，直接返回
        if Some(node_ptr) == self.tail {
            return;
        }

        // 1. 断开 node_ptr 与其前后节点的连接
        unsafe {
            let prev = (*node_ptr.as_ptr()).prev;
            let next = (*node_ptr.as_ptr()).next;

            if let Some(p) = prev {
                (*p.as_ptr()).next = next;
            } else {
                // node_ptr 是头节点
                self.head = next;
            }

            if let Some(n) = next {
                (*n.as_ptr()).prev = prev;
            } else {
                // node_ptr 是尾节点
                self.tail = prev;
            }
        }

        // 2. 将 node_ptr 插入到尾节点之后
        unsafe {
            if let Some(tail) = self.tail {
                (*tail.as_ptr()).next = Some(node_ptr);
                (*node_ptr.as_ptr()).prev = Some(tail);
                (*node_ptr.as_ptr()).next = None;
            }
        }

        // 3. 更新尾节点指针
        self.tail = Some(node_ptr);
    }


    //删除头节点 并返回头节点的key
    pub fn pop_front(&mut self) -> Option<Arc<String>> {
        // 1. 使用 .take() 来“取出” head，这会自动把 self.head 设为 None
        //    这比 if let 更安全，因为它处理了所有权
        let head_ptr = self.head.take()?; // 如果是 None，直接 ? 退出返回 None

        // 走到这里，说明 head_ptr 是 Some(ptr)，并且 self.head 已经是 None 了
        self.len -= 1;

        // 2. 把裸指针变回 Box，我们现在“拥有”了这个节点
        //    这是整个函数里最关键的 unsafe 操作
        let head_box: Box<Node> = unsafe { Box::from_raw(head_ptr.as_ptr()) };

        // 3. 处理“新的头节点” (head_box.next)
        match head_box.next {
            Some(mut new_head) => {
                // 3a. 【边界情况1：链表里还有节点】
                // 新的头是 new_head
                // 我们必须把它的 prev 设为 None
                unsafe {
                    new_head.as_mut().prev = None;
                }
                // 把 self.head 指向这个新头
                self.head = Some(new_head);
            }
            None => {
                // 3b. 【你刚意识到的那个边界！(head_box.next 是 None)】
                // 这说明我们刚弹出了“最后一个”节点
                // self.head 已经是 None (因为第1步的 take())
                // 我们现在必须手动把 tail 也设为 None！
                self.tail = None;
            }
        }

        // 4. 返回我们“拥有”的那个被弹出的节点
        Some(head_box.key)
    }

    //删除指定节点
    pub fn pop_node(&mut self, node_ptr: NonNull<Node>) {
        self.len -= 1;
        unsafe {
            let prev = (*node_ptr.as_ptr()).prev;
            let next = (*node_ptr.as_ptr()).next;

            if let Some(p) = prev {
                (*p.as_ptr()).next = next;
            } else {
                // node_ptr 是头节点
                self.head = next;
            }

            if let Some(n) = next {
                (*n.as_ptr()).prev = prev;
            } else {
                // node_ptr 是尾节点
                self.tail = prev;
            }

            // 释放节点内存
            let _ = Box::from_raw(node_ptr.as_ptr());
        }
    }
}
