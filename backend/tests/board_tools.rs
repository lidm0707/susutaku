use async_trait::async_trait;
use backend::domain::{BoardOp, BoardRequest, BoardResult, ToolCall, ToolKind, ToolSet};
use backend::port::outbound::BoardOps;

#[test]
fn board_tool_permissions_are_per_kind() {
    // each board-family name grants only its own kind
    let set = ToolSet::from_names(&["board".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Board));
    assert!(!set.allows(ToolKind::Card));
    assert!(!set.allows(ToolKind::Pipeline));
    assert!(!set.allows(ToolKind::Routine));

    let set = ToolSet::from_names(&[
        "card".to_string(),
        "pipeline".to_string(),
        "routine".to_string(),
    ])
    .unwrap();
    assert!(!set.allows(ToolKind::Board));
    assert!(set.allows(ToolKind::Card));
    assert!(set.allows(ToolKind::Pipeline));
    assert!(set.allows(ToolKind::Routine));
    assert!(set.any_board());

    let set = ToolSet::from_names(&["search".to_string()]).unwrap();
    assert!(!set.any_board());
    assert!(ToolSet::from_names(&["kanban".to_string()]).is_err());
}

#[test]
fn parses_board_tool_calls() {
    assert_eq!(
        ToolCall::parse("TOOL: BOARD_LIST"),
        Some(ToolCall::BoardList)
    );
    assert_eq!(
        ToolCall::parse("</think>\nTOOL: PIPELINE_CREATE nightly-report"),
        Some(ToolCall::PipelineCreate {
            name: "nightly-report".into(),
            spec: None
        })
    );
    assert_eq!(
        ToolCall::parse(
            r#"TOOL: PIPELINE_CREATE nightly {"nodes":[{"id":"a","stage":"search","params":{"query":"trump"}}],"links":[]}"#
        ),
        Some(ToolCall::PipelineCreate {
            name: "nightly".into(),
            spec: Some(
                r#"{"nodes":[{"id":"a","stage":"search","params":{"query":"trump"}}],"links":[]}"#
                    .into()
            )
        })
    );
    assert_eq!(ToolCall::parse("TOOL: PIPELINE_CREATE"), None);
    assert_eq!(
        ToolCall::parse("TOOL: PIPELINE_CREATE nightly "),
        Some(ToolCall::PipelineCreate {
            name: "nightly".into(),
            spec: None
        })
    );
    assert_eq!(
        ToolCall::parse(
            "<invoke name=\"pipeline_create\"><parameter name=\"name\">nightly</parameter><parameter name=\"spec\">   </parameter></invoke>"
        ),
        Some(ToolCall::PipelineCreate {
            name: "nightly".into(),
            spec: None
        })
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
        ToolCall::parse("TOOL: CARD_ROUTINE 7 0 */5 * * *"),
        Some(ToolCall::CardSchedule {
            card_id: 7,
            cron: "0 */5 * * *".into()
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_LINK 7 2"),
        Some(ToolCall::CardLink {
            card_id: 7,

            pipeline_id: 2
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
        Some(ToolCall::PipelineCreate {
            name: "nightly".into(),
            spec: None
        })
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
            BoardOp::CreatePipeline { name, spec } => {
                Ok(format!("pipeline 1 created: {name} spec={spec:?}"))
            }
            _ => Ok("ok".into()),
        }
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
            op: BoardOp::CreatePipeline {
                name: "nightly".into(),
                spec: None,
            },
        })
        .await;
    assert_eq!(out.unwrap(), "pipeline 1 created: nightly spec=None");
}
