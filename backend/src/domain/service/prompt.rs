//! Domain service: chat prompt assembly and context formatting.

use crate::domain::valueobject::search_result::SearchResult;

/// Hard cap on search results fed into a prompt.
pub const CONTEXT_RESULTS_MAX: usize = 5;
pub const CONTEXT_HEADER: &str = "Web search results:\n";
pub const CONTEXT_FOOTER: &str = "\n\nUse the results above when relevant. Question: ";

/// Tool-use protocol (Zed-style): the model calls tools by starting its reply
/// with a TOOL: line. Max tool rounds before a forced final answer.
pub const TOOL_ROUNDS_MAX: usize = 2;
pub const TOOL_INSTRUCTION: &str = "You HAVE web tools and MUST use them when the question involves current/web information:\n- TOOL: SEARCH <query> - search the web\n- TOOL: FETCH <url> - read a web page\n- TOOL: SHELL <cmd> - run a shell command inside the agent sandbox\nIf a tool would help, reply with ONLY one tool line (like: TOOL: SEARCH apple mlx). The system runs it and gives you results. Never say you cannot access the web. Otherwise answer directly.\n\n";
pub const BOARD_TOOL_INSTRUCTION: &str = "You ALSO have kanban board tools for scheduling recurring work:\n- TOOL: BOARD_LIST - list projects, pipelines and cards with their ids\n- TOOL: PIPELINE_CREATE <name> - create an empty pipeline\n- TOOL: CARD_CREATE <project_id> <title> - create a kanban card\n- TOOL: CARD_LINK <card_id> <pipeline_id> - attach a pipeline to a card\n- TOOL: CARD_SCHEDULE <card_id> <cron> - set a 5-field UTC cron on a card (e.g. 0 */5 * * * runs every 5 hours); the scheduler runs the card's pipeline when due\nTo fulfill a request like \"run X every 5 hours\": create the pipeline, create the card, link them, then set the schedule. Reply with ONLY one tool line per turn.\n\n";
pub const TOOL_RESULT_HEADER: &str = "\n\nTool results:\n";

/// Assemble the final model prompt: tool offer (optional), context (optional), question.
pub struct Prompt;

impl Prompt {
    pub fn build(message: &str, context: &str, offer_tools: bool, offer_board: bool) -> String {
        let mut body = String::new();
        if offer_tools {
            body.push_str(TOOL_INSTRUCTION);
            if offer_board {
                body.push_str(BOARD_TOOL_INSTRUCTION);
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
