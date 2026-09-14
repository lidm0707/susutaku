//! Trace of one tool call made during a chat turn.

use super::ToolKind;

use crate::domain::TOOL_DENIED as TOOL_DENIED_NOTE;

pub const TOOL_SUMMARY_MAX: usize = 200;

/// File extensions recognized as image artifacts.
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
/// File extensions recognized as text artifacts.
const TEXT_EXTENSIONS: [&str; 6] = ["txt", "json", "csv", "md", "log", "html"];

/// What a tool call produced on disk: a picture, readable text, or any other
/// file. Paths are workspace-relative; the artifact targets a kanban card as
/// a card resource when the turn carries a card id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub kind: ArtifactKind,
    /// Workspace-relative path of the produced file.
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Image,
    Text,
    File,
}

impl ArtifactKind {
    /// Classify by file extension; unknown extensions stay generic files.
    pub fn from_path(path: &str) -> Self {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            Self::Image
        } else if TEXT_EXTENSIONS.contains(&ext.as_str()) {
            Self::Text
        } else {
            Self::File
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "text",
            Self::File => "file",
        }
    }
}

/// Live tool-progress event streamed to the UI while the agent loop runs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolEvent {
    pub kind: String,
    pub input: String,
    pub ok: bool,
    pub summary: String,
}

impl From<&ToolUse> for ToolEvent {
    fn from(u: &ToolUse) -> Self {
        Self {
            kind: format!("{:?}", u.kind).to_lowercase(),
            input: u.input.clone(),
            ok: u.ok,
            summary: u.summary.clone(),
        }
    }
}

/// What the agent did with a tool: which one, with what input, and a short
/// result summary. Shown to the user; never fed back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUse {
    pub kind: ToolKind,
    pub input: String,
    pub ok: bool,
    pub summary: String,
    /// Files the call produced; attached to the target card as resources.
    pub artifacts: Vec<Artifact>,
}

impl ToolUse {
    pub fn new(kind: ToolKind, input: &str, result: &Result<String, String>) -> Self {
        let (ok, body) = match result {
            Ok(s) => (true, s.as_str()),
            Err(e) => (false, e.as_str()),
        };
        Self {
            kind,
            input: input.chars().take(TOOL_SUMMARY_MAX).collect(),
            ok,
            summary: body.chars().take(TOOL_SUMMARY_MAX).collect(),
            artifacts: Vec::new(),
        }
    }

    /// Record a produced file on this call, classified by extension.
    pub fn with_artifact(mut self, path: &str) -> Self {
        self.artifacts.push(Artifact {
            kind: ArtifactKind::from_path(path),
            path: path.to_string(),
        });
        self
    }

    /// A call the tool allow-list refused.
    pub fn denied(kind: ToolKind, input: &str) -> Self {
        Self {
            kind,
            input: input.chars().take(TOOL_SUMMARY_MAX).collect(),
            ok: false,
            summary: TOOL_DENIED_NOTE.to_string(),
            artifacts: Vec::new(),
        }
    }
}
