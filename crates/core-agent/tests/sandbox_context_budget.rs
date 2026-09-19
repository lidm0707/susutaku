use core_agent::podman::sandbox::Sandbox;

/// Over-budget pushes evict the oldest transcript entries (token budget,
/// not a count cap): the first small entry must be gone while the newest
/// entry survives, and the kept set stays under the default budget.
#[test]
fn context_budget_evicts_oldest_when_full() {
    let sb = Sandbox::new().expect("sandbox");

    let small = "first message";
    sb.push_context(core_agent::sandbox_abstract_layer::Role::User, small);

    let big = "x".repeat(10_000 * latenspace::CHARS_PER_TOKEN);
    for i in 0..3 {
        sb.push_context(
            core_agent::sandbox_abstract_layer::Role::User,
            format!("{i} {big}"),
        );
    }

    let transcript = sb.transcript();
    assert!(
        transcript.iter().all(|e| !e.content.contains(small)),
        "oldest entry should be evicted"
    );
    let last = transcript.last().expect("at least one entry kept");
    assert!(last.content.starts_with('2'), "newest entry survives");

    // Budget holds except for the min-kept rule: one entry larger than the
    // whole budget cannot be evicted (that would empty the transcript).
    if transcript.len() > latenspace::MIN_ENTRIES_KEPT {
        let tokens: usize = transcript
            .iter()
            .map(|e| latenspace::estimate_tokens(&e.content))
            .sum();
        assert!(tokens <= latenspace::DEFAULT_MAX_TOKENS);
    }

    sb.purge();
}
