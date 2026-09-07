use queue_rs::{BoundedQueue, UnboundedQueue};

#[test]
fn unbounded_fifo_order() {
    let q = UnboundedQueue::new();
    q.push(1);
    q.push(2);
    q.push(3);
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), Some(3));
    assert_eq!(q.pop(), None);
    assert!(q.is_empty());
}

#[test]
fn bounded_rejects_when_full() {
    let q = BoundedQueue::new(2);
    assert!(q.push(1).is_ok());
    assert!(q.push(2).is_ok());
    assert_eq!(q.push(3), Err(3));
    assert_eq!(q.len(), 2);
    assert_eq!(q.capacity(), 2);
}

#[test]
fn bounded_frees_slot_after_pop() {
    let q = BoundedQueue::new(1);
    assert!(q.push("a").is_ok());
    assert_eq!(q.pop(), Some("a"));
    assert!(q.push("b").is_ok());
}

#[test]
fn default_capacity() {
    let q: BoundedQueue<u8> = BoundedQueue::default();
    assert_eq!(q.capacity(), 1024);
}

#[test]
fn mpsc_across_threads() {
    use std::sync::Arc;
    use std::thread;

    const PRODUCERS: usize = 4;
    const PER_PRODUCER: usize = 250;

    let q = Arc::new(UnboundedQueue::new());
    let mut handles = Vec::new();
    for p in 0..PRODUCERS {
        let q = Arc::clone(&q);
        handles.push(thread::spawn(move || {
            for i in 0..PER_PRODUCER {
                q.push(p * PER_PRODUCER + i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(q.len(), PRODUCERS * PER_PRODUCER);

    let mut sum = 0u64;
    while let Some(v) = q.pop() {
        sum += v as u64;
    }
    let total: u64 = (0..PRODUCERS * PER_PRODUCER).map(|v| v as u64).sum();
    assert_eq!(sum, total);
}
