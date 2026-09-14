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
pub const TOOL_CODING_INSTRUCTION: &str = "You HAVE a coding tool for writing project code. To create or overwrite a file in the project work tree, reply with ONLY this block (the only case where XML is allowed; all other tools use TOOL: lines):\n<invoke name=\"coding\"><parameter name=\"path\">relative/path/to/file.ext</parameter><parameter name=\"code\">the complete file content</parameter></invoke>\n";
pub const TOOL_MATH_INSTRUCTION: &str = "You HAVE a geometry math tool (exact computation, no estimation):\n- TOOL: MATH distance x1 y1 x2 y2 (or: distance x1,y1 x2,y2) - euclidean distance\n- TOOL: MATH triangle base height - triangle area\n- TOOL: MATH heron a b c - triangle area from three sides\n- TOOL: MATH circle radius - area and circumference\n- TOOL: MATH polygon x1,y1 x2,y2 ... - polygon area (shoelace, 3+ points)\n- TOOL: MATH pythag a b - hypotenuse\n- TOOL: MATH haversine lat1 lon1 lat2 lon2 - great-circle distance in km\n";
pub const TOOL_GIT_INSTRUCTION: &str = "You HAVE a git tool for version control. It runs on the HOST (has network access), unlike SHELL:\n- TOOL: GIT CLONE <url> - clone a repo into the agent work tree (only a fresh, empty work tree)\n- TOOL: GIT STATUS - HEAD + dirty/clean of the work tree\n- TOOL: GIT DIFF - patch of the work tree changes\n";
pub const TOOL_RULES: &str = "If a tool would help, reply with ONLY one tool line (like: TOOL: SEARCH apple mlx). The system runs it and gives you results. Never say you cannot access the web. Otherwise answer directly. NEVER use XML, JSON or function-call syntax for tools (no <invoke>, no <tool_call>) — only plain TOOL: lines are understood.\n\n";
pub const BOARD_TOOL_INSTRUCTION: &str = "You ALSO have kanban board tools for setting up routines (recurring work; survives sandbox shutdown because the backend scheduler runs it on the host):\n- TOOL: BOARD_LIST - list projects, pipelines and cards with their ids\n- TOOL: PIPELINE_CREATE <name> [spec-json] - create a pipeline; spec is optional JSON: {\"nodes\":[{\"id\":\"a\",\"stage\":\"fetch\",\"params\":{\"url\":\"https://...\"}},...],\"links\":[{\"from\":\"a\",\"to\":\"b\"}]} - every node needs id, stage and ALL required params of its stage; without spec an empty pipeline is created (add nodes later)\n- TOOL: CARD_CREATE <project_id> <title> - create a kanban card\n- TOOL: CARD_LINK <card_id> <pipeline_id> - attach a pipeline to a card\n- TOOL: CARD_ROUTINE <card_id> <cron> - give a card a routine: a 5-field UTC cron (e.g. 0 */5 * * * runs every 5 hours); the backend scheduler runs the card's pipeline when due. There is no standalone \"routine\" object — routines are card schedules.\nMODIFYING an existing card (user names a card, or asks to change/clear a routine): do NOT create anything. First call BOARD_LIST to find the card id by its title, then call CARD_ROUTINE with that id and the requested cron; create/attach a pipeline ONLY if the card has none and the user asked for new recurring work.\nFor brand-new recurring work: create the pipeline (with a valid spec), create the card, link them, then set the routine. Reply with ONLY one tool line per turn.\n\n";
pub const PIPELINE_SCHEMA_HEADER: &str =
    "Pipeline node stages (stage — what it does; params). Required params MUST be set:\n";
pub const TOOL_RESULT_HEADER: &str = "\n\nTool results:\n";
pub const TOOL_DENIED: &str = "tool denied: this agent is not permitted to use that tool";

/// Assemble the final model prompt: tool offer (optional), context (optional), question.
pub struct Prompt;

/// Prompt sections rendered in order; each decides for itself whether it
/// applies to the agent's tool set.
enum Section {
    Web,
    Shell,
    Rules,
    Coding,
    Board,
    Math,
    Git,
}

impl Section {
    const ALL: [Self; 7] = [
        Self::Web,
        Self::Shell,
        Self::Rules,
        Self::Coding,
        Self::Board,
        Self::Math,
        Self::Git,
    ];

    fn render(&self, tools: &ToolSet, offer_board: bool) -> Option<String> {
        match self {
            Self::Web if tools.allows(ToolKind::Search) || tools.allows(ToolKind::Fetch) => {
                Some(TOOL_WEB_INSTRUCTION.to_owned())
            }
            Self::Shell if tools.allows(ToolKind::Shell) => Some(TOOL_SHELL_INSTRUCTION.to_owned()),
            Self::Rules if tools.allows(ToolKind::Shell) || Self::Web.fits(tools) => {
                Some(TOOL_RULES.to_owned())
            }
            Self::Coding if tools.allows(ToolKind::Coding) => {
                Some(TOOL_CODING_INSTRUCTION.to_owned())
            }
            Self::Board if offer_board && tools.allows(ToolKind::Board) => Some(format!(
                "{BOARD_TOOL_INSTRUCTION}{}{PIPELINE_SCHEMA_HEADER}\n\n",
                piplines::port::schema_text()
            )),
            Self::Math if tools.allows(ToolKind::Math) => Some(TOOL_MATH_INSTRUCTION.to_owned()),
            Self::Git if tools.allows(ToolKind::Git) => Some(TOOL_GIT_INSTRUCTION.to_owned()),
            _ => None,
        }
    }

    fn fits(&self, tools: &ToolSet) -> bool {
        match self {
            Self::Web => tools.allows(ToolKind::Search) || tools.allows(ToolKind::Fetch),
            _ => false,
        }
    }
}

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
            for section in Section::ALL {
                if let Some(text) = section.render(tools, offer_board) {
                    body.push_str(&text);
                }
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
