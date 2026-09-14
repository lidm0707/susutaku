pub mod fetch;
pub mod git_in_sandbox;
pub mod kanban;
pub mod lsp;
pub mod web_search;

/// Agent-timeout for every outbound HTTP call — the tool harness must never
/// hang the agent loop on a dead host.
pub const HTTP_TIMEOUT_SECS: u64 = 30;

/// A shared ureq agent with the harness timeout applied.
pub fn http() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
}

/// The agent's tool surface: one enum, one dispatch point (no hard-coded
/// call sites per tool).
#[derive(Debug, Clone)]
pub enum Tool {
    Fetch {
        url: String,
    },
    WebSearch {
        query: String,
    },
    Definition {
        path: String,
        text: String,
        line: u32,
        col: usize,
    },
    CreateCard {
        project_id: i64,
        title: String,
        description: Option<String>,
    },
}

impl Tool {
    pub const fn name(&self) -> &'static str {
        match self {
            Tool::Fetch { .. } => "fetch",
            Tool::WebSearch { .. } => "web_search",
            Tool::Definition { .. } => "definition",
            Tool::CreateCard { .. } => "create_card",
        }
    }

    /// Execute the tool and return its model-facing text output.
    pub fn run(&self) -> Result<String, String> {
        match self {
            Tool::Fetch { url } => fetch::fetch(url),
            Tool::WebSearch { query } => {
                web_search::search(query).map(|r| SearchResultList(r).to_string())
            }
            Tool::Definition {
                path,
                text,
                line,
                col,
            } => lsp::definition(lsp::DEFAULT_ROOT, path, text, *line, *col),
            Tool::CreateCard {
                project_id,
                title,
                description,
            } => kanban::create_card(
                &kanban::base_url(),
                kanban::token().as_deref(),
                &kanban::NewCard {
                    project_id: *project_id,
                    title: title.clone(),
                    description: description.clone(),
                },
            ),
        }
    }
}

struct SearchResultList(Vec<web_search::SearchResult>);

impl std::fmt::Display for SearchResultList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", web_search::SearchResult::summarize(&self.0))
    }
}
