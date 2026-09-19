//! Agent config table entity: write-side shape for `agent_configs`.

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfigDraft {
    pub name: String,
    pub model: String,
    pub persona: String,
    pub prompt: String,
    pub output: String,
    /// Tool allow-list (search|fetch|shell|board); empty = all tools.
    pub allowed_tools: Vec<String>,
    /// False = the agent must never receive images (screenshots, attachments).
    pub receive_images: bool,
    /// Reasoning depth, one of ThinkLevel::as_str ("off"|"low"|"medium"|"high").
    pub thinking: String,
    /// Context-window budget in tokens.
    pub ctx_limit: i64,
    /// Full-context behaviour, one of CtxPolicy::as_str
    /// ("warn"|"new_thread"|"keep_going").
    pub ctx_policy: String,
}
