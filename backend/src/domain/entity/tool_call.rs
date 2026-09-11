//! Entity: a tool invocation parsed out of a model reply. It is the object
//! the chat use case acts upon in the agentic loop.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Search(String),
    Fetch(String),
    Shell(String),
    CardCreate { project_id: i64, title: String },
    CardSchedule { card_id: i64, cron: String },
    CardLink { card_id: i64, pipeline_id: i64 },
    PipelineCreate(String),
    BoardList,
}

const TOOL_PREFIX: &str = "TOOL:";
const TOOL_SEARCH: &str = "SEARCH";
const TOOL_FETCH: &str = "FETCH";
const TOOL_SHELL: &str = "SHELL";
const TOOL_CARD_CREATE: &str = "CARD_CREATE";
const TOOL_CARD_SCHEDULE: &str = "CARD_SCHEDULE";
const TOOL_CARD_LINK: &str = "CARD_LINK";
const TOOL_PIPELINE_CREATE: &str = "PIPELINE_CREATE";
const TOOL_BOARD_LIST: &str = "BOARD_LIST";

impl ToolCall {
    /// First TOOL: line after the </think> block, if any. The argument may be
    /// empty (e.g. `TOOL: BOARD_LIST`).
    pub fn parse(reply: &str) -> Option<Self> {
        let visible = reply
            .split_once("</think>")
            .map(|(_, after)| after)
            .unwrap_or(reply);
        let line = visible
            .lines()
            .map(str::trim_start)
            .find(|l| l.to_uppercase().starts_with(TOOL_PREFIX))?;
        let rest = line[TOOL_PREFIX.len()..].trim();
        let (kind, arg) = match rest.split_once(' ') {
            Some((kind, arg)) => (kind, arg.trim()),
            None => (rest, ""),
        };
        match kind.to_uppercase().as_str() {
            TOOL_SEARCH => Some(Self::Search(arg.to_string())),
            TOOL_FETCH => Some(Self::Fetch(arg.to_string())),
            TOOL_SHELL => Some(Self::Shell(arg.to_string())),
            TOOL_CARD_CREATE => {
                let (project_id, title) = arg.split_once(' ')?;
                Some(Self::CardCreate {
                    project_id: project_id.parse().ok()?,
                    title: title.to_string(),
                })
            }
            TOOL_CARD_SCHEDULE => {
                let (card_id, cron) = arg.split_once(' ')?;
                Some(Self::CardSchedule {
                    card_id: card_id.parse().ok()?,
                    cron: cron.to_string(),
                })
            }
            TOOL_CARD_LINK => {
                let (card_id, pipeline_id) = arg.split_once(' ')?;
                Some(Self::CardLink {
                    card_id: card_id.parse().ok()?,
                    pipeline_id: pipeline_id.parse().ok()?,
                })
            }
            TOOL_PIPELINE_CREATE => Some(Self::PipelineCreate(arg.to_string())),
            TOOL_BOARD_LIST => Some(Self::BoardList),
            _ => None,
        }
    }
}
