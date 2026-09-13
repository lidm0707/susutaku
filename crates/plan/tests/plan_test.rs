use plan::{Plan, Priority, Status};

#[test]
fn add_assigns_sequential_ids() {
    let mut p = Plan::new("demo");
    let a = p.add("first");
    let b = p.add("second");
    assert_eq!(a, 1);
    assert_eq!(b, 2);
    assert_eq!(p.len(), 2);
}

#[test]
fn runnable_respects_dependencies() {
    let mut p = Plan::new("demo");
    let a = p.add("a");
    let b = p.add_with(|id| {
        let mut t = plan::Task::new(id, "b");
        t.depends_on = vec![a];
        t
    });
    assert_eq!(p.runnable(), vec![a]);
    p.set_status(a, Status::Done);
    assert_eq!(p.runnable(), vec![b]);
}

#[test]
fn progress_counts_done() {
    let mut p = Plan::new("demo");
    let a = p.add("a");
    p.add("b");
    assert_eq!(p.progress(), 0.0);
    p.set_status(a, Status::Done);
    assert!((p.progress() - 0.5).abs() < 1e-9);
    assert_eq!(Plan::new("empty").progress(), 0.0);
}

#[test]
fn priority_ordering() {
    let mut p = Plan::new("demo");
    let low = p.add_with(|id| {
        let mut t = plan::Task::new(id, "low");
        t.priority = Priority::Low;
        t
    });
    let crit = p.add_with(|id| {
        let mut t = plan::Task::new(id, "crit");
        t.priority = Priority::Critical;
        t
    });
    assert_eq!(p.by_priority(), vec![crit, low]);
    assert!(Priority::Critical > Priority::Low);
    assert!(Status::Done > Status::Todo);
}

#[test]
fn blocked_not_runnable() {
    let mut p = Plan::new("demo");
    let a = p.add_with(|id| {
        let mut t = plan::Task::new(id, "a");
        t.status = Status::Blocked;
        t
    });
    assert!(p.runnable().is_empty());
    p.set_status(a, Status::Todo);
    assert_eq!(p.runnable(), vec![a]);
}
