//! Domain layer: chat prompt assembly and web-search routing policy.
//! No I/O, no framework types.

//! Domain layer: chat prompt assembly and web-search routing policy.
//! No I/O, no framework types.

/// Hard cap on search results fed into a prompt.
pub const CONTEXT_RESULTS_MAX: usize = 5;
pub const CONTEXT_HEADER: &str = "Web search results:\n";
pub const CONTEXT_FOOTER: &str = "\n\nUse the results above when relevant. Question: ";

/// Tool-use protocol (Zed-style): the model calls tools by starting its reply
/// with a TOOL: line. Max tool rounds before a forced final answer.
pub const TOOL_ROUNDS_MAX: usize = 2;
pub const TOOL_INSTRUCTION: &str = "You HAVE web tools and MUST use them when the question involves current/web information:\n- TOOL: SEARCH <query> - search the web\n- TOOL: FETCH <url> - read a web page\n- TOOL: SHELL <cmd> - run a shell command inside the agent sandbox\nIf a tool would help, reply with ONLY one tool line (like: TOOL: SEARCH apple mlx). The system runs it and gives you results. Never say you cannot access the web. Otherwise answer directly.\n\n";
pub const TOOL_RESULT_HEADER: &str = "\n\nTool results:\n";
const TOOL_PREFIX: &str = "TOOL:";
const TOOL_SEARCH: &str = "SEARCH";
const TOOL_FETCH: &str = "FETCH";
const TOOL_SHELL: &str = "SHELL";

/// A tool invocation parsed out of a model reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Search(String),
    Fetch(String),
    Shell(String),
}

impl ToolCall {
    /// First TOOL: line after the </think> block, if any.
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
        let (kind, arg) = rest.split_once(' ')?;
        let arg = arg.trim();
        match kind.to_uppercase().as_str() {
            TOOL_SEARCH => Some(Self::Search(arg.to_string())),
            TOOL_FETCH => Some(Self::Fetch(arg.to_string())),
            TOOL_SHELL => Some(Self::Shell(arg.to_string())),
            _ => None,
        }
    }
}

/// A single web result to ground a prompt with.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// How search is engaged: the model routes itself (Auto), always (Force), never (Off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Off,
    Auto,
    Force,
}

impl SearchMode {
    /// "on" = always search, "off" = never, absent/"auto" = model routes itself.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("on") => Self::Force,
            Some("off") => Self::Off,
            _ => Self::Auto,
        }
    }

    /// Wrap a message so the model answers it. The architecture-specific
    /// chat template is applied by the inference engine (it knows the arch);
    /// this only frames the body as a single user turn.
    pub fn wrap(message: &str) -> String {
        message.to_string()
    }
}

/// Assemble the final model prompt: tool offer (optional), context (optional), question.
pub struct Prompt;

impl Prompt {
    pub fn build(message: &str, context: &str, offer_tools: bool) -> String {
        let mut body = String::new();
        if offer_tools {
            body.push_str(TOOL_INSTRUCTION);
        }
        if !context.is_empty() {
            body.push_str(context);
            body.push('\n');
        }
        body.push_str(message);
        SearchMode::wrap(&body)
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
