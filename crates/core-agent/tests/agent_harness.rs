use core_agent::agent_state::{AgentState, Phase, Role};
use core_agent::toolcall::Tool;

#[test]
fn tool_names_are_stable() {
    let tools = [
        Tool::Fetch {
            url: "https://x.test".into(),
        },
        Tool::WebSearch { query: "q".into() },
        Tool::Definition {
            path: "a.rs".into(),
            text: String::new(),
            line: 0,
            col: 0,
        },
    ];
    assert_eq!(tools[0].name(), "fetch");
    assert_eq!(tools[1].name(), "web_search");
    assert_eq!(tools[2].name(), "definition");
}

#[test]
fn agent_records_tool_cycle() {
    let state = AgentState::new();
    state.apply_tool("fetch", "https://x.test", "page text");
    assert_eq!(state.phase(), Phase::Done);
    let t = state.transcript();
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].role, Role::User);
    assert!(t[0].content.contains("[fetch] https://x.test"));
    assert_eq!(t[1].role, Role::Tool);
    assert_eq!(t[1].content, "page text");
}

#[test]
fn history_is_bounded_by_max() {
    let state = AgentState::new();
    for i in 0..(core_agent::agent_state::MAX_HISTORY + 16) {
        state.apply_tool("web_search", &format!("q{i}"), "out");
    }
    assert_eq!(
        state.transcript().len(),
        core_agent::agent_state::MAX_HISTORY
    );
}
