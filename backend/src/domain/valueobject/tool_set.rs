//! Tool permission set: which tools one chat agent may use.

/// Tools an agent can be granted. Order matters only for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Search,
    Fetch,
    Shell,
    Board,
}

pub const TOOL_KIND_NAMES: [(&str, ToolKind); 4] = [
    ("search", ToolKind::Search),
    ("fetch", ToolKind::Fetch),
    ("shell", ToolKind::Shell),
    ("board", ToolKind::Board),
];

/// Allow-list of tools for one agent. An empty allow-list means every tool
/// is permitted; a non-empty list permits only the listed kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolSet {
    pub search: bool,
    pub fetch: bool,
    pub shell: bool,
    pub board: bool,
}

impl ToolSet {
    /// Everything allowed (no per-agent restriction).
    pub const fn all() -> Self {
        Self {
            search: true,
            fetch: true,
            shell: true,
            board: true,
        }
    }

    /// Build from tool names. Empty = all. Unknown names are an error so
    /// typos never silently widen or narrow permissions.
    pub fn from_names(names: &[String]) -> Result<Self, String> {
        if names.is_empty() {
            return Ok(Self::all());
        }
        let mut set = Self::default();
        for name in names {
            let name = name.trim();
            let (_, kind) = TOOL_KIND_NAMES
                .iter()
                .find(|(n, _)| *n == name)
                .ok_or_else(|| format!("unknown tool {name:?} (use search|fetch|shell|board)"))?;
            match kind {
                ToolKind::Search => set.search = true,
                ToolKind::Fetch => set.fetch = true,
                ToolKind::Shell => set.shell = true,
                ToolKind::Board => set.board = true,
            }
        }
        Ok(set)
    }

    pub const fn allows(&self, kind: ToolKind) -> bool {
        match kind {
            ToolKind::Search => self.search,
            ToolKind::Fetch => self.fetch,
            ToolKind::Shell => self.shell,
            ToolKind::Board => self.board,
        }
    }
}
