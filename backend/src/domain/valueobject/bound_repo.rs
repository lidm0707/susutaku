//! The git repo bound to a chat's project: executor-only credentials plus a
//! credential-free URL safe to show the model.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRepo {
    pub project_id: i64,
    /// Full clone URL; may embed credentials — never rendered into a prompt.
    pub url: String,
    pub secret: Option<String>,
    /// Credential-free URL for prompts and tool traces.
    pub display_url: String,
}
