//! Domain service: chat prompt assembly and context formatting.

use crate::domain::valueobject::search_result::SearchResult;
use crate::domain::valueobject::tool_set::{TOOL_KIND_NAMES, ToolKind, ToolSet};

/// Hard cap on search results fed into a prompt.
pub const CONTEXT_RESULTS_MAX: usize = 5;
pub const CONTEXT_HEADER: &str = "Web search results:\n";
pub const CONTEXT_FOOTER: &str = "\n\nUse the results above when relevant. Question: ";

/// Tool-use protocol (Zed-style): the model calls tools by starting its reply
/// with a TOOL: line. Max tool rounds before a forced final answer.
pub const TOOL_ROUNDS_MAX: usize = 8;
pub const TOOL_WEB_INSTRUCTION: &str = "You HAVE web tools and MUST use them when the question involves current/web information:\n- TOOL: SEARCH <query> - search the web\n- TOOL: FETCH <url> - read a web page\n";
pub const TOOL_SHELL_INSTRUCTION: &str =
    "- TOOL: SHELL <cmd> - run a shell command inside the agent sandbox\n";
pub const TOOL_CODING_INSTRUCTION: &str = "You HAVE a coding tool for writing project code. To create or overwrite a file in the project work tree, reply with ONLY this block (the only case where XML is allowed; all other tools use TOOL: lines):\n<invoke name=\"coding\"><parameter name=\"path\">relative/path/to/file.ext</parameter><parameter name=\"code\">the complete file content</parameter></invoke>\n";
pub const PLOT_INSTRUCTION: &str = "When the user asks to plot, graph or visualize a function or equation, reply with a ```plot fenced block instead of ASCII art or matplotlib code. The block body is either JSON: {\"exprs\":[\"2/3*x - 1/3\"],\"x\":[-6,6],\"y\":[-4,4]} (y optional), or one expression per line (e.g. sin(x)). Use * for multiplication, ^ for power; the browser renders the graph for you — never draw graphs as text. Always solve for y first: an expression must not contain '=' (write 3 - x, never x+y=3).\n";
pub const TOOL_MATH_INSTRUCTION: &str = "You HAVE a geometry math tool (exact computation, no estimation):\n- TOOL: MATH distance x1 y1 x2 y2 (or: distance x1,y1 x2,y2) - euclidean distance\n- TOOL: MATH triangle base height - triangle area\n- TOOL: MATH heron a b c - triangle area from three sides\n- TOOL: MATH circle radius - area and circumference\n- TOOL: MATH polygon x1,y1 x2,y2 ... - polygon area (shoelace, 3+ points)\n- TOOL: MATH pythag a b - hypotenuse\n- TOOL: MATH haversine lat1 lon1 lat2 lon2 - great-circle distance in km\n";
pub const TOOL_GIT_INSTRUCTION: &str = "You HAVE a git tool for version control. CLONE/STATUS/DIFF run on the HOST; BRANCH/COMMIT/PUSH/PR run INSIDE your own sandbox container (they need a named agent session and a repo bound in settings → git repos):\n- TOOL: GIT CLONE [url] - clone a repo into the agent work tree (only a fresh, empty work tree); with no url, the repo bound to this chat's project is used\n- TOOL: GIT STATUS - HEAD + dirty/clean of the work tree\n- TOOL: GIT DIFF - patch of the work tree changes\n- TOOL: GIT BRANCH <name> - create and switch to a new branch\n- TOOL: GIT COMMIT <message> - stage all changes and commit them\n- TOOL: GIT PUSH <branch> - push the current work to <branch> on the bound remote\n- TOOL: GIT PR <title>[ | <base>] - open a pull request from the current branch (base defaults to main)\nBRANCH/COMMIT/PUSH/PR select the agent container by naming it LAST on the line: TOOL: GIT COMMIT my message @<agent>. Without @<agent> they run only when the chat itself is bound to an agent.\n";
pub const TOOL_LSP_INSTRUCTION: &str = "You HAVE a language-server tool (rust-analyzer) for exact code navigation in the project work tree. line and col are 0-based; path is workspace-relative and must not contain spaces:\n- TOOL: LSP DEFINITION <path> <line> <col> - where the symbol at that position is defined\n- TOOL: LSP REFERENCES <path> <line> <col> - every use of that symbol\n- TOOL: LSP HOVER <path> <line> <col> - type and docs at that position\nPrefer these over grepping when you need to locate or understand a symbol.\n";
pub const TOOL_AGENT_RUN_INSTRUCTION: &str = "You HAVE an agent-run tool for making a NAMED agent do real work in its own sandbox (its work tree is what the review page shows):\n- TOOL: AGENT_RUN <agent> <cmd> - run one shell command as that agent (spawns it on demand); the command output comes back to you\nUse this whenever the user asks an agent (by name) to actually DO something: write/edit files, build, test, clone, commit. Do not print commands for the user to run — run them yourself with this tool, one command per turn.\n";
pub const TOOL_RULES: &str = "If a tool would help, reply with ONLY one tool line (like: TOOL: SEARCH apple mlx). The system runs it and gives you results. Never say you cannot access the web. Otherwise answer directly. NEVER use XML, JSON or function-call syntax for tools (no <invoke>, no <tool_call>) — only plain TOOL: lines are understood.\n\n";
pub const BOARD_TOOL_INSTRUCTION: &str = "You ALSO have task board tools. A CARD is the unit of work: it carries its assigned agent, an optional image and an optional routine (recurring schedule); the backend scheduler runs routine cards on the host:\n- TOOL: CARD_FIND <query> - find cards whose title or description contain the query words; each hit prints its card id, project, agent, image and cron\n- TOOL: BOARD_LIST - list projects and cards with their ids\n- TOOL: CARD_CREATE <project_id> <title> [| <description>] - create a task card. The title is a SHORT summary (max ~6 words); after ` | ` put a description that captures the task/data the card is about (goal, steps, context). When the user asks to create/save a card — or writes out card content — you MUST create it with CARD_CREATE; NEVER paste card content into your reply as markdown instead of creating the card. project_id is a NUMERIC id: when you don't know it, call BOARD_LIST first — NEVER write a placeholder word like `project` as an id.\n- TOOL: CARD_AGENT <card_id> <agent_name> - assign the agent that runs the card. EVERY card MUST get an agent before it can run — always do this right after CARD_CREATE; an agentless card FAILS on every run. card_id is the NUMERIC id CARD_CREATE printed.\n- TOOL: CARD_IMAGE <card_id> <image> - set the card's sandbox image (container image used by the run); `CARD_IMAGE <card_id> clear` removes it. Only when the user asks for a specific image.\n- TOOL: CARD_ROUTINE <card_id> <cron> - give a card a routine: a 5-field UTC cron (e.g. 0 */5 * * * runs every 5 hours); the scheduler runs the card's agent when due. There is no standalone \"routine\" object — routines are card schedules.\n- TOOL: CARD_ROUTINE_CLEAR <card_id> - remove a card's routine (it stops recurring; the card and its agent stay).\n- TOOL: CARD_RUN <card_id> - run the card's assigned agent ONCE right now and get the result back. Use when the user says \"run it/do it now\"; it is NOT needed after setting a routine.\nAfter each setup step report back in one short line: what was created/found (card id), the assigned agent, the routine cron if any, and what happens next (runs now / runs on schedule).\nMODIFYING an existing card (user names a card, or asks to change/clear a routine or agent): do NOT create anything. First call BOARD_LIST to find the card id by its title, then call the relevant tool with that id.\nFor brand-new recurring work: CARD_CREATE the card, CARD_AGENT it, then CARD_ROUTINE it with the cron. Reply with ONLY one tool line per turn.\nWHEN THE USER ASKS YOU TO DO A TASK: first CARD_FIND with 2-4 topic words of the task. If a matching card exists, reuse its id; otherwise CARD_CREATE a new card whose description holds the plan (goal, steps, context). Then ALWAYS CARD_AGENT the card — the agent's own tools (shell, fetch, search, git, …) do the work, no pipeline needed. Only if the work must run on a schedule, also CARD_ROUTINE the card with a cron; to run it right now, CARD_RUN it. Reply with ONLY one tool line per turn.\nACT, DON'T ASK: when the user asks to add/build/change/create something, their message IS the task spec — start the flow above immediately (CARD_CREATE → CARD_AGENT → CARD_RUN now; default: no routine). NEVER reply by asking what the task is, which agent to use (use the chat's bound CHAT AGENT), or whether to schedule; fill gaps with reasonable assumptions from the user's own words.\n\n";
pub const TOOL_RESULT_HEADER: &str = "\n\nTool results:\n";
pub const TOOL_DENIED: &str = "tool denied: this agent is not permitted to use that tool";
pub const SKILL_CONTEXT_OPEN: &str = "<context>\n";
pub const SKILL_CONTEXT_CLOSE: &str = "\n</context>\n\n";

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
    Lsp,
    Agent,
}

impl Section {
    const ALL: [Self; 9] = [
        Self::Web,
        Self::Shell,
        Self::Rules,
        Self::Coding,
        Self::Board,
        Self::Math,
        Self::Git,
        Self::Lsp,
        Self::Agent,
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
            Self::Board if offer_board && tools.any_board() => {
                Some(BOARD_TOOL_INSTRUCTION.to_owned())
            }
            Self::Math if tools.allows(ToolKind::Math) => Some(TOOL_MATH_INSTRUCTION.to_owned()),
            Self::Git if tools.allows(ToolKind::Git) => Some(TOOL_GIT_INSTRUCTION.to_owned()),
            Self::Lsp if tools.allows(ToolKind::Lsp) => Some(TOOL_LSP_INSTRUCTION.to_owned()),
            Self::Agent if tools.allows(ToolKind::Agent) => {
                Some(TOOL_AGENT_RUN_INSTRUCTION.to_owned())
            }
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
    /// Skill sheets for every enabled tool, in `TOOL_KIND_NAMES` order.
    fn skills(tools: &ToolSet) -> String {
        TOOL_KIND_NAMES
            .iter()
            .filter(|(_, kind)| tools.allows(*kind))
            .filter_map(|(name, _)| core_agent::skills::skill(name))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn build(
        message: &str,
        context: &str,
        offer_tools: bool,
        offer_board: bool,
        tools: &ToolSet,
    ) -> String {
        let mut body = String::from(PLOT_INSTRUCTION);
        if offer_tools {
            let skills = Self::skills(tools);
            if !skills.is_empty() {
                body.push_str(SKILL_CONTEXT_OPEN);
                body.push_str(&skills);
                body.push_str(SKILL_CONTEXT_CLOSE);
            }
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
