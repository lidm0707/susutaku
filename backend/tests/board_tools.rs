#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use async_trait::async_trait;
use backend::domain::{BoardOp, BoardRequest, BoardResult, ToolCall, ToolKind, ToolSet};
use backend::port::outbound::BoardOps;

#[test]
fn malformed_tool_line_is_still_an_offer() {
    // the screenshot bug: a non-numeric project id must not dead-end silently
    let line = "TOOL: CARD_CREATE project Sandbox quick echo test | echo hello";
    assert!(ToolCall::offers(line));
    assert_eq!(ToolCall::parse(line), None);
    assert!(ToolCall::offers("blah\nTOOL: BOARD_LIST"));
    assert!(ToolCall::offers("</think>TOOL: CARD_RUN"));
    assert!(!ToolCall::offers("just an answer, no tool"));
    assert!(!ToolCall::offers("<think>TOOL: SEARCH x</think>done"));
}

#[test]
fn board_tool_permissions_are_per_kind() {
    // each board-family name grants only its own kind
    let set = ToolSet::from_names(&["board".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Board));
    assert!(!set.allows(ToolKind::Card));
    assert!(!set.allows(ToolKind::Routine));

    let set = ToolSet::from_names(&["card".to_string(), "routine".to_string()]).unwrap();
    assert!(!set.allows(ToolKind::Board));
    assert!(set.allows(ToolKind::Card));
    assert!(set.allows(ToolKind::Routine));
    assert!(set.any_board());

    let set = ToolSet::from_names(&["search".to_string()]).unwrap();
    assert!(!set.any_board());
    assert!(ToolSet::from_names(&["task".to_string()]).is_err());
}

#[test]
fn legacy_pipeline_tool_name_is_ignored() {
    let set = ToolSet::from_names(&["card".to_string(), "pipeline".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Card));
    assert!(!set.allows(ToolKind::Routine));
}

#[test]
fn parses_board_tool_calls() {
    assert_eq!(
        ToolCall::parse("TOOL: BOARD_LIST"),
        Some(ToolCall::BoardList)
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_CREATE 3 run backups"),
        Some(ToolCall::CardCreate {
            project_id: 3,
            title: "run backups".into(),
            description: None
        })
    );
    assert_eq!(
        ToolCall::parse(
            "TOOL: CARD_CREATE 3 quota research | YouTube: 10k units/day; TikTok ~1k/day"
        ),
        Some(ToolCall::CardCreate {
            project_id: 3,
            title: "quota research".into(),
            description: Some("YouTube: 10k units/day; TikTok ~1k/day".into())
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_AGENT 7 nightly"),
        Some(ToolCall::CardAgent {
            card_id: 7,
            agent: "nightly".into()
        })
    );
    assert_eq!(ToolCall::parse("TOOL: CARD_AGENT 7"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_AGENT 7  "), None);
    assert_eq!(
        ToolCall::parse("TOOL: CARD_IMAGE 7 debian-slim"),
        Some(ToolCall::CardImage {
            card_id: 7,
            image: Some("debian-slim".into())
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_IMAGE 7 clear"),
        Some(ToolCall::CardImage {
            card_id: 7,
            image: None
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_ROUTINE 7 0 */5 * * *"),
        Some(ToolCall::CardSchedule {
            card_id: 7,
            cron: "0 */5 * * *".into()
        })
    );

    assert_eq!(
        ToolCall::parse("TOOL: CARD_FIND quota research"),
        Some(ToolCall::CardFind {
            query: "quota research".into()
        })
    );
    assert_eq!(ToolCall::parse("TOOL: CARD_FIND"), None);
    assert_eq!(
        ToolCall::parse("TOOL: CARD_RUN 7"),
        Some(ToolCall::CardRun { card_id: 7 })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_ROUTINE_CLEAR 7"),
        Some(ToolCall::CardRoutineClear { card_id: 7 })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_ROUTINE 7 clear"),
        Some(ToolCall::CardRoutineClear { card_id: 7 })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_ROUTINE 7 CLEAR"),
        Some(ToolCall::CardRoutineClear { card_id: 7 })
    );
    assert_eq!(ToolCall::parse("TOOL: CARD_RUN"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_ROUTINE_CLEAR x"), None);
    assert_eq!(ToolCall::parse("TOOL: PIPELINE_CREATE nightly"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_LINK 7 2"), None);
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"card_run\"><parameter name=\"card_id\">6</parameter></invoke>"
        ),
        Some(ToolCall::CardRun { card_id: 6 })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"card_agent\"><parameter name=\"card_id\">6</parameter><parameter name=\"agent\">nightly</parameter></invoke>"
        ),
        Some(ToolCall::CardAgent {
            card_id: 6,
            agent: "nightly".into()
        })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"card_image\"><parameter name=\"card_id\">6</parameter><parameter name=\"image\">debian-slim</parameter></invoke>"
        ),
        Some(ToolCall::CardImage {
            card_id: 6,
            image: Some("debian-slim".into())
        })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"card_routine_clear\"><parameter name=\"card_id\">6</parameter></invoke>"
        ),
        Some(ToolCall::CardRoutineClear { card_id: 6 })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"find_card\"><parameter name=\"query\">quota</parameter></invoke>"
        ),
        Some(ToolCall::CardFind {
            query: "quota".into()
        })
    );
}

#[test]
fn malformed_board_tool_call_is_none() {
    assert_eq!(ToolCall::parse("TOOL: CARD_CREATE oops no number"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_CREATE 3"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_CREATE 3  | desc"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_ROUTINE 7"), None);
}

#[test]
fn parses_xml_invoke_fallback() {
    let xml = "<invoke name=\"card_routine\">\n<parameter name=\"card_id\">6</parameter>\n<parameter name=\"cron\">0 * * * *</parameter>\n</invoke>";
    assert_eq!(
        ToolCall::parse(xml),
        Some(ToolCall::CardSchedule {
            card_id: 6,
            cron: "0 * * * *".into()
        })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"search\"><parameter name=\"query\">mlx rust</parameter></invoke>"
        ),
        Some(ToolCall::Search("mlx rust".into()))
    );
    assert_eq!(
        ToolCall::parse("<invoke name=\"board_list\"></invoke>"),
        Some(ToolCall::BoardList)
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"pipeline_create\"><parameter name=\"name\">nightly</parameter></invoke>"
        ),
        None
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"create_card\"><parameter name=\"project_id\">1</parameter><parameter name=\"name\">dancing with my code new content</parameter></invoke>"
        ),
        Some(ToolCall::CardCreate {
            project_id: 1,
            title: "dancing with my code new content".into(),
            description: None
        })
    );
    assert_eq!(
        ToolCall::parse("<invoke name=\"card_routine\"></invoke>"),
        None
    );
}

struct FakeBoard;

#[async_trait]
impl BoardOps for FakeBoard {
    async fn exec(&self, req: BoardRequest) -> BoardResult {
        if req.token.is_none() {
            return Err("forbidden: editor role required".into());
        }
        match req.op {
            BoardOp::AssignAgent { card_id, agent } => {
                Ok(format!("agent {agent} assigned to card {card_id}"))
            }
            _ => Ok("ok".into()),
        }
    }

    async fn card_context(&self, _card_id: i64) -> Option<String> {
        None
    }
}

#[tokio::test]
async fn board_port_denies_missing_token() {
    let board = FakeBoard;
    let out = board
        .exec(BoardRequest {
            token: None,
            op: BoardOp::Summary,
        })
        .await;
    assert!(out.is_err());
}

#[tokio::test]
async fn board_port_executes_with_token() {
    let board = FakeBoard;
    let out = board
        .exec(BoardRequest {
            token: Some("tok".into()),
            op: BoardOp::AssignAgent {
                card_id: 3,
                agent: "nightly".into(),
            },
        })
        .await;
    assert_eq!(out.unwrap(), "agent nightly assigned to card 3");
}
