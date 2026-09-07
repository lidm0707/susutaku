use std::collections::VecDeque;
use std::sync::RwLock;

pub struct UnboundedQueue<T> {
    inner: RwLock<VecDeque<T>>,
}

impl<T> UnboundedQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(VecDeque::new()),
        }
    }

    pub fn push(&self, item: T) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.push_back(item);
    }

    pub fn pop(&self) -> Option<T> {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.pop_front()
    }

    pub fn len(&self) -> usize {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for UnboundedQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}
