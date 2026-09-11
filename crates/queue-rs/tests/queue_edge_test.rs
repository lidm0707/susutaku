use queue_rs::{BoundedQueue, UnboundedQueue};

#[test]
fn unbounded_default_is_empty() {
    let q: UnboundedQueue<u8> = UnboundedQueue::default();
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);
    q.push(1);
    assert!(!q.is_empty());
    assert_eq!(q.len(), 1);
}

#[test]
fn bounded_is_empty_transitions() {
    let q: BoundedQueue<u8> = BoundedQueue::default();
    assert!(q.is_empty());
    q.push(1).unwrap();
    assert!(!q.is_empty());
    q.pop();
    assert!(q.is_empty());
}

#[test]
fn bounded_with_default_capacity() {
    const DEFAULT_CAPACITY: usize = 1024;
    let q = BoundedQueue::<u8>::with_default_capacity();
    assert_eq!(q.capacity(), DEFAULT_CAPACITY);
    assert!(q.is_empty());
}

#[test]
fn bounded_fill_then_drain_in_order() {
    const CAP: usize = 3;
    let q = BoundedQueue::new(CAP);
    for i in 0..CAP {
        q.push(i).unwrap();
    }
    for i in 0..CAP {
        assert_eq!(q.pop(), Some(i));
    }
    assert_eq!(q.pop(), None);
}

#[test]
#[should_panic(expected = "capacity must be non-zero")]
fn bounded_zero_capacity_panics() {
    let _ = BoundedQueue::<u8>::new(0);
}
