//! Integration tests for the kat tool-call gate: JSON-stuck classification,
//! repair prompts, and the transcript pruner's eviction.

use latenspace::{JsonStuck, ToolPruner, check_json, repair_prompt};

#[test]
fn valid_json_object_parses() {
    let v = check_json("{\"tool\": \"shell\", \"cmd\": \"cargo check\"}").expect("valid");
    assert_eq!(v["tool"], "shell");
}

#[test]
fn fenced_json_with_prose_parses() {
    let reply = "Here is the call:\n```json\n{\"tool\": \"git\"}\n```\ndone";
    let v = check_json(reply).expect("fenced object");
    assert_eq!(v["tool"], "git");
}

#[test]
fn object_surrounded_by_prose_parses() {
    let v = check_json("sure: {\"a\": 1} thanks").expect("embedded object");
    assert_eq!(v["a"], 1);
}

#[test]
fn truncated_object_is_stuck() {
    let reply = "{\"tool\": \"shell\", \"cmd\": \"cargo ch";
    match check_json(reply) {
        Err(JsonStuck::Truncated { depth }) => assert!(depth >= 1),
        other => panic!("expected truncation, got {other:?}"),
    }
}

#[test]
fn truncation_inside_string_is_stuck() {
    let reply = "{\"a\": \"unfinished";
    assert!(matches!(
        check_json(reply),
        Err(JsonStuck::Truncated { .. })
    ));
}

#[test]
fn complete_but_invalid_json_is_invalid() {
    match check_json("{\"a\": 1,}") {
        Err(JsonStuck::Invalid { detail }) => {
            assert!(detail.contains("trailing comma"), "detail: {detail}");
        }
        other => panic!("expected invalid, got {other:?}"),
    }
}

#[test]
fn closer_without_opener_is_invalid_not_truncated() {
    // katgpt-validator rule: `}` with no `{` must never false-accept.
    assert!(matches!(
        check_json("} oops {"),
        Err(JsonStuck::Invalid { .. })
    ));
}

#[test]
fn policy_flags_unknown_actions_but_known_verbs_pass() {
    let policy = latenspace::Policy::new();
    let good = check_json("{\"tool\": \"git\", \"action\": \"commit\"}").expect("valid");
    assert!(policy.violations(&good).is_empty(), "git/commit are known");

    let verb =
        check_json("{\"tool\": \"shell\", \"action\": \"compile the crate\"}").expect("valid");
    assert!(
        policy.violations(&verb).is_empty(),
        "compile is a corpus verb"
    );

    let bad = check_json("{\"tool\": \"shlel\", \"action\": \"frumigate the crate\"}")
        .expect("valid json, off-policy");
    let mut v = policy.violations(&bad);
    v.sort();
    assert_eq!(v, vec!["frumigate".to_owned(), "shlel".to_owned()]);
    let hint = latenspace::policy_prompt(&v);
    assert!(hint.contains("shlel"), "hint: {hint}");
    assert!(hint.contains("unknown action"));
}

#[test]
fn policy_ignores_non_action_keys_and_non_string_values() {
    let policy = latenspace::Policy::new();
    let value =
        check_json("{\"path\": \"shlel.rs\", \"count\": 3, \"cmd\": \"whatever\"}").expect("valid");
    assert!(policy.violations(&value).is_empty());
}

#[test]
fn policy_from_actions_restricts_the_allowlist() {
    let policy = latenspace::Policy::from_actions(&["shell"]);
    let value = check_json("{\"tool\": \"git\"}").expect("valid");
    assert_eq!(policy.violations(&value), vec!["git".to_owned()]);
}

#[test]
fn global_policy_is_built_once_and_checks() {
    let value = check_json("{\"tool\": \"shell\"}").expect("valid");
    assert!(latenspace::global_policy().violations(&value).is_empty());
}

#[test]
fn non_object_json_is_not_object() {
    assert_eq!(check_json("[1, 2, 3]"), Err(JsonStuck::NotObject));
}

#[test]
fn no_json_at_all_is_no_json() {
    assert_eq!(
        check_json("plain prose reply, nothing structured"),
        Err(JsonStuck::NoJson)
    );
    assert_eq!(check_json(""), Err(JsonStuck::NoJson));
}

#[test]
fn repair_prompt_names_the_defect() {
    let stuck = JsonStuck::Truncated { depth: 2 };
    let hint = repair_prompt(&stuck);
    assert!(hint.contains("2 unclosed bracket"), "hint: {hint}");
    assert!(hint.contains("Repeat the complete call"));
    assert!(repair_prompt(&JsonStuck::NoJson).contains("no JSON object"));
    let invalid = JsonStuck::Invalid {
        detail: "EOF while parsing an object at line 1 column 12".to_owned(),
    };
    assert!(repair_prompt(&invalid).contains("syntactically invalid"));
    let expected = JsonStuck::Invalid {
        detail: "trailing comma at line 1 column 12, expected ident".to_owned(),
    };
    let hint = repair_prompt(&expected);
    assert!(hint.contains("expected ident"), "hint: {hint}");
}

#[test]
fn escaped_quotes_inside_strings_do_not_confuse_the_scan() {
    let reply = "{\"cmd\": \"echo \\\"hi\\\"\"}";
    assert!(check_json(reply).is_ok());
    let cut = "{\"cmd\": \"say \\\"hi";
    assert!(matches!(check_json(cut), Err(JsonStuck::Truncated { .. })));
}

#[test]
fn pruner_evicts_old_outputs_under_budget() {
    let big = "x".repeat(400);
    let mut pruner = ToolPruner::new(300);
    pruner.append("SHELL round-1", &big);
    pruner.append("SHELL round-2", &big);
    pruner.append("SHELL round-3", &big);
    // Budget 300 tokens, each entry ~100+: pushing round-3 exceeds it and
    // evicts the coldest entry (round-1).
    let rendered = pruner.render();
    assert!(!rendered.contains("round-1"), "oldest round evicted");
    assert!(rendered.contains("round-3"), "latest round kept");
}

#[test]
fn pruner_never_evicts_pinned_entries() {
    let big = "y".repeat(400);
    let mut pruner = ToolPruner::new(400);
    pruner.pin("goal", "ship the feature");
    for round in 1..=4 {
        pruner.append("SHELL", &format!("round-{round} {big}"));
    }
    let rendered = pruner.render();
    assert!(
        rendered.contains("ship the feature"),
        "pinned entry survives"
    );
}

#[test]
fn pruner_reports_usage() {
    let mut pruner = ToolPruner::new(4096);
    assert!(pruner.is_empty());
    pruner.append("SHELL ls", "file.rs");
    assert_eq!(pruner.len(), 1);
    assert!(pruner.used_tokens() > 0);
}
