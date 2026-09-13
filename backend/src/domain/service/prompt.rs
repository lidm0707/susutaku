//! Domain service: chat prompt assembly and context formatting.

use crate::domain::valueobject::search_result::SearchResult;
use crate::domain::valueobject::tool_set::{ToolKind, ToolSet};

/// Hard cap on search results fed into a prompt.
pub const CONTEXT_RESULTS_MAX: usize = 5;
pub const CONTEXT_HEADER: &str = "Web search results:\n";
pub const CONTEXT_FOOTER: &str = "\n\nUse the results above when relevant. Question: ";

/// Tool-use protocol (Zed-style): the model calls tools by starting its reply
/// with a TOOL: line. Max tool rounds before a forced final answer.
pub const TOOL_ROUNDS_MAX: usize = 2;
pub const TOOL_WEB_INSTRUCTION: &str = "You HAVE web tools and MUST use them when the question involves current/web information:\n- TOOL: SEARCH <query> - search the web\n- TOOL: FETCH <url> - read a web page\n";
pub const TOOL_SHELL_INSTRUCTION: &str =
    "- TOOL: SHELL <cmd> - run a shell command inside the agent sandbox\n";
pub const TOOL_RULES: &str = "If a tool would help, reply with ONLY one tool line (like: TOOL: SEARCH apple mlx). The system runs it and gives you results. Never say you cannot access the web. Otherwise answer directly. NEVER use XML, JSON or function-call syntax for tools (no <invoke>, no <tool_call>) — only plain TOOL: lines are understood.\n\n";
pub const BOARD_TOOL_INSTRUCTION: &str = "You ALSO have kanban board tools for setting up routines (recurring work; survives sandbox shutdown because the backend scheduler runs it on the host):\n- TOOL: BOARD_LIST - list projects, pipelines and cards with their ids\n- TOOL: PIPELINE_CREATE <name> [spec-json] - create a pipeline; spec is optional JSON: {\"nodes\":[{\"id\":\"a\",\"stage\":\"fetch\",\"params\":{\"url\":\"https://...\"}},...],\"links\":[{\"from\":\"a\",\"to\":\"b\"}]} - every node needs id, stage and ALL required params of its stage; without spec an empty pipeline is created (add nodes later)\n- TOOL: CARD_CREATE <project_id> <title> - create a kanban card\n- TOOL: CARD_LINK <card_id> <pipeline_id> - attach a pipeline to a card\n- TOOL: CARD_ROUTINE <card_id> <cron> - give a card a routine: a 5-field UTC cron (e.g. 0 */5 * * * runs every 5 hours); the backend scheduler runs the card's pipeline when due. There is no standalone \"routine\" object — routines are card schedules.\nMODIFYING an existing card (user names a card, or asks to change/clear a routine): do NOT create anything. First call BOARD_LIST to find the card id by its title, then call CARD_ROUTINE with that id and the requested cron; create/attach a pipeline ONLY if the card has none and the user asked for new recurring work.\nFor brand-new recurring work: create the pipeline (with a valid spec), create the card, link them, then set the routine. Reply with ONLY one tool line per turn.\n\n";
pub const PIPELINE_SCHEMA_HEADER: &str =
    "Pipeline node stages (stage — what it does; params). Required params MUST be set:\n";
pub const TOOL_RESULT_HEADER: &str = "\n\nTool results:\n";
pub const TOOL_DENIED: &str = "tool denied: this agent is not permitted to use that tool";

/// Assemble the final model prompt: tool offer (optional), context (optional), question.
pub struct Prompt;

impl Prompt {
    pub fn build(
        message: &str,
        context: &str,
        offer_tools: bool,
        offer_board: bool,
        tools: &ToolSet,
    ) -> String {
        let mut body = String::new();
        if offer_tools {
            if tools.allows(ToolKind::Search) || tools.allows(ToolKind::Fetch) {
                body.push_str(TOOL_WEB_INSTRUCTION);
            }
            if tools.allows(ToolKind::Shell) {
                body.push_str(TOOL_SHELL_INSTRUCTION);
                body.push_str(TOOL_RULES);
            } else if !body.is_empty() {
                body.push_str(TOOL_RULES);
            }
            if offer_board && tools.allows(ToolKind::Board) {
                body.push_str(BOARD_TOOL_INSTRUCTION);
                body.push_str(PIPELINE_SCHEMA_HEADER);
                body.push_str(&piplines::port::schema_text());
                body.push_str("\n\n");
            }
        }
        if !context.is_empty() {
            body.push_str(context);
            body.push('\n');
        }
        body.push_str(message);
        crate::domain::valueobject::search_mode::SearchMode::wrap(&body)
    }

    /// Formatted search results for prompt context.
    pub fn format_results(message: &str, results: &[SearchResult]) -> String {
        if results.is_empty() {
            return String::new();
        }
        let n = results.len().min(CONTEXT_RESULTS_MAX);
        let lines = results[..n]
            .iter()
            .map(|r| format!("{} | {} | {}", r.title, r.url, r.snippet))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{CONTEXT_HEADER}{lines}{CONTEXT_FOOTER}{message}")
    }
}
