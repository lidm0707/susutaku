use latenspace::{Context, Entry, EntryKind, MIN_ENTRIES_KEPT};

#[test]
fn render_touches_entries_and_marks_headers() {
    let mut ctx = Context::new(1024);
    ctx.push(Entry::new(EntryKind::System, "you are susutaku", 4).pinned());
    ctx.push(Entry::new(EntryKind::Focus, "card fix-login", 3));
    ctx.push(Entry::new(EntryKind::Memory, "prior chat tail", 3));

    let out = ctx.render();
    assert!(out.contains("[system]\nyou are susutaku"));
    assert!(out.contains("[focus]\ncard fix-login"));
    assert_eq!(ctx.used_tokens(), 10);
}

#[test]
fn budget_evicts_lowest_usage_rate_first_and_spares_pinned() {
    let mut ctx = Context::new(10);
    ctx.push(Entry::new(EntryKind::System, "s", 3).pinned());
    ctx.push(Entry::new(EntryKind::Memory, "a", 3));
    let b = ctx.push(Entry::new(EntryKind::Memory, "b", 3));

    ctx.touch(b); // b used once, a never used → a has the lowest rate
    ctx.push(Entry::new(EntryKind::Thread, "c", 3));

    fn texts(ctx: &Context) -> Vec<&str> {
        ctx.entries_ref().iter().map(|e| e.text.as_str()).collect()
    }
    let t = texts(&ctx);
    assert!(!t.contains(&"a"), "cold entry evicted, got {t:?}");
    assert!(t.contains(&"b"), "warm entry kept, got {t:?}");
    assert!(t.contains(&"s"), "pinned entry kept, got {t:?}");

    // Down to one oversize entry: never evicts below MIN_ENTRIES_KEPT.
    ctx.push(Entry::new(EntryKind::Thread, "huge", 100));
    assert!(ctx.len() >= MIN_ENTRIES_KEPT);
}

#[test]
fn touch_unknown_id_is_false() {
    let mut ctx = Context::new(64);
    assert!(!ctx.touch(999));
}
