use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use work::services::schedule_task::{Schedule, Scheduler};

#[test]
fn interval_task_fires_repeatedly() {
    let scheduler = Arc::new(Scheduler::new());
    let count = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&count);
    scheduler.register(
        "tick",
        Schedule::Interval(1),
        Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }),
    );
    std::thread::sleep(Duration::from_millis(1400));
    assert!(count.load(Ordering::Relaxed) >= 1);
}

#[test]
fn once_task_fires_then_removed() {
    let scheduler = Scheduler::new();
    let fired = Arc::new(RwLock::new(false));
    let flag = Arc::clone(&fired);
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let id = scheduler.register(
        "once",
        Schedule::Once(at),
        Box::new(move || {
            *flag.write().expect("lock") = true;
        }),
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(*fired.read().expect("lock"));
    assert!(!scheduler.cancel(id), "once task should auto-remove");
    assert!(!scheduler.cancel(9999));
}
