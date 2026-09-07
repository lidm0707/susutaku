use std::collections::VecDeque;
use std::sync::RwLock;

const DEFAULT_CAPACITY: usize = 1024;

pub struct BoundedQueue<T> {
    inner: RwLock<Inner<T>>,
    capacity: usize,
}

struct Inner<T> {
    items: VecDeque<T>,
}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be non-zero");
        Self {
            inner: RwLock::new(Inner {
                items: VecDeque::with_capacity(capacity),
            }),
            capacity,
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.items.len() >= self.capacity {
            return Err(item);
        }
        guard.items.push_back(item);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.items.pop_front()
    }

    pub fn len(&self) -> usize {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Default for BoundedQueue<T> {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}
