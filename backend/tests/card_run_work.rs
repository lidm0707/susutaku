//! Pure-helper tests for the card-run work-tree mode: the path-escape guard,
//! the slot key, the prompt build and the bound-repo op resolution.

use backend::app::card_run::{
    BoundRepo, WorkStep, bind_op, resolve_in_tree, slot_key, task_branch, work_prompt, work_step,
};
use backend::domain::GitOp;
use std::path::Path;
use task_rs::{AgentConfigRow, CardRow};

fn bound() -> BoundRepo {
    BoundRepo {
        slot: "dev#7".into(),
        agent: "dev".into(),
        task: "7".into(),
        url: "https://git.example.com/acme/repo".into(),
        token: "tok".into(),
    }
}

#[test]
fn resolve_in_tree_rejects_escape() {
    let root = Path::new("/tmp/work-tree-root");
    let rel = resolve_in_tree(root, "src/main.rs").expect("normal relative path");
    assert_eq!(rel, root.join("src/main.rs"));
    for bad in [
        "/etc/passwd",
        "../escape.txt",
        "a/../../escape.txt",
        ".",
        "./x",
    ] {
        assert!(resolve_in_tree(root, bad).is_err(), "must reject {bad}");
    }
}

#[test]
fn slot_key_joins_agent_and_card() {
    assert_eq!(slot_key("dev", 42), "dev#42");
}

#[test]
fn bind_op_fills_bound_repo() {
    let b = bound();
    let op = GitOp::Push {
        branch: "task/x".into(),
        url: None,
        token: None,
    };
    match bind_op(op, &b) {
        GitOp::Push { url, token, .. } => {
            assert_eq!(url.as_deref(), Some("https://git.example.com/acme/repo"));
            assert_eq!(token.as_deref(), Some("tok"));
        }
        other => panic!("wrong op {other:?}"),
    }
    let explicit = GitOp::Clone {
        url: Some("https://other.example.com/a/b".into()),
        token: Some("own".into()),
    };
    match bind_op(explicit, &b) {
        GitOp::Clone { url, token } => {
            assert_eq!(url.as_deref(), Some("https://other.example.com/a/b"));
            assert_eq!(token.as_deref(), Some("own"));
        }
        other => panic!("wrong op {other:?}"),
    }
}

fn work_cfg() -> AgentConfigRow {
    AgentConfigRow {
        id: 1,
        name: "dev".into(),
        model: String::new(),
        persona: "tester".into(),
        prompt: String::new(),
        output: String::new(),
        allowed_tools: Vec::new(),
        receive_images: false,
        thinking: "off".into(),
        ctx_limit: 128000,
        ctx_policy: "compact".into(),
    }
}

#[test]
fn work_prompt_carries_task_and_hint() {
    let cfg = work_cfg();
    let card = CardRow {
        id: 7,
        column_id: "todo".into(),
        project_id: Some(1),
        title: "fix the bug".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: Some("dev".into()),
        agent_state: None,
        run_status: String::new(),
        last_agent: None,
        last_run_id: None,
        assignee: None,
        cron: None,
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
        image: None,
    };
    let prompt = work_prompt(
        &cfg,
        &card,
        "Tool results:\n- SHELL ls\n",
        &[],
        "task/7-dev",
    );
    assert!(prompt.contains("persona: tester"));
    assert!(prompt.contains("task:"));
    assert!(prompt.contains("fix the bug"));
    assert!(prompt.contains("Tool results:"));
    assert!(!prompt.contains("project skills:"));
}

#[test]
fn work_prompt_embeds_attached_skills() {
    let cfg = work_cfg();
    let card = CardRow {
        id: 7,
        column_id: "todo".into(),
        project_id: Some(1),
        title: "fix the bug".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: Some("dev".into()),
        agent_state: None,
        run_status: String::new(),
        last_agent: None,
        last_run_id: None,
        assignee: None,
        cron: None,
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
        image: None,
    };
    let skills = vec![task_rs::SkillRow {
        id: 1,
        name: "susutaku-project".into(),
        body: "cargo check && cargo clippy".into(),
    }];
    let prompt = work_prompt(&cfg, &card, "", &skills, "task/7-dev");
    assert!(prompt.contains("project skills:"));
    assert!(prompt.contains("## susutaku-project"));
    assert!(prompt.contains("cargo check && cargo clippy"));
    assert!(prompt.ends_with("cargo check && cargo clippy\n\n") || prompt.contains("clippy\n"));
}

#[test]
fn task_branch_is_deterministic() {
    assert_eq!(task_branch("zai", "35"), "task/35-zai");
    assert_eq!(task_branch("z ai", "3/5"), "task/3_5-z_ai");
}

#[test]
fn work_prompt_names_the_task_branch() {
    let cfg = work_cfg();
    let card = CardRow {
        id: 35,
        column_id: "todo".into(),
        project_id: Some(1),
        title: "verify push".into(),
        description: String::new(),
        priority: "normal".into(),
        position: 0,
        agent_name: Some("zai".into()),
        agent_state: None,
        run_status: String::new(),
        last_agent: None,
        last_run_id: None,
        assignee: None,
        cron: None,
        deadline: None,
        labels: None,
        checklist: None,
        estimate: None,
        image: None,
    };
    let prompt = work_prompt(&cfg, &card, "", &[], "task/35-zai");
    // the model must see the exact branch and never invent one like
    // test-push-branch; PR head/base must match the publish step
    assert!(prompt.contains("`task/35-zai`"));
    assert!(prompt.contains("`main` <- `task/35-zai`"));
    assert!(prompt.contains("GIT BRANCH is denied"));
    assert!(!prompt.contains("test-push-branch"));
}

#[test]
fn work_step_retries_malformed_tool_offer() {
    // well-formed tool call -> executed
    assert!(matches!(
        work_step("TOOL: SHELL ls".to_string()),
        WorkStep::Tool(_)
    ));
    // tool offered but malformed -> retry with repair hint, run continues
    assert!(matches!(
        work_step("TOOL: CARD_CREATE title only".to_string()),
        WorkStep::Retry
    ));
    // no tool offer -> final answer, run ends
    assert!(matches!(
        work_step("all done, tests pass".to_string()),
        WorkStep::Final(_)
    ));
}
