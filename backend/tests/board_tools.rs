use async_trait::async_trait;
use backend::domain::{BoardOp, BoardRequest, BoardResult, ToolCall};
use backend::port::outbound::BoardOps;

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
        ToolCall::parse("TOOL: CARD_CREATE 3 run backups"),
        Some(ToolCall::CardCreate {
            project_id: 3,
            title: "run backups".into()
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: CARD_SCHEDULE 7 0 */5 * * *"),
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
}

#[test]
fn malformed_board_tool_call_is_none() {
    assert_eq!(ToolCall::parse("TOOL: CARD_CREATE oops no number"), None);
    assert_eq!(ToolCall::parse("TOOL: CARD_SCHEDULE 7"), None);
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
