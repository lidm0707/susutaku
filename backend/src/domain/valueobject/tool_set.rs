//! Tool permission set: which tools one chat agent may use.

/// Tools an agent can be granted. Order matters only for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Search,
    Fetch,
    Shell,
    Board,
    Card,
    Pipeline,
    Routine,
    Coding,
    Math,
    Git,
    Lsp,
}

pub const TOOL_KIND_NAMES: [(&str, ToolKind); 11] = [
    ("search", ToolKind::Search),
    ("fetch", ToolKind::Fetch),
    ("shell", ToolKind::Shell),
    ("board", ToolKind::Board),
    ("card", ToolKind::Card),
    ("pipeline", ToolKind::Pipeline),
    ("routine", ToolKind::Routine),
    ("coding", ToolKind::Coding),
    ("math", ToolKind::Math),
    ("git", ToolKind::Git),
    ("lsp", ToolKind::Lsp),
];

/// Allow-list of tools for one agent. An empty allow-list means every tool
/// is permitted; a non-empty list permits only the listed kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolSet {
    pub search: bool,
    pub fetch: bool,
    pub shell: bool,
    pub board: bool,
    pub card: bool,
    pub pipeline: bool,
    pub routine: bool,
    pub coding: bool,
    pub math: bool,
    pub git: bool,
    pub lsp: bool,
}

impl ToolSet {
    /// Everything allowed (no per-agent restriction).
    pub const fn all() -> Self {
        Self {
            search: true,
            fetch: true,
            shell: true,
            board: true,
            card: true,
            pipeline: true,
            routine: true,
            coding: true,
            math: true,
            git: true,
            lsp: true,
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
                .ok_or_else(|| {
                    format!(
                        "unknown tool {name:?} (use search|fetch|shell|board|card|pipeline|routine|coding|math|git|lsp)"
                    )
                })?;
            match kind {
                ToolKind::Search => set.search = true,
                ToolKind::Fetch => set.fetch = true,
                ToolKind::Shell => set.shell = true,
                ToolKind::Board => set.board = true,
                ToolKind::Card => set.card = true,
                ToolKind::Pipeline => set.pipeline = true,
                ToolKind::Routine => set.routine = true,
                ToolKind::Coding => set.coding = true,
                ToolKind::Math => set.math = true,
                ToolKind::Git => set.git = true,
                ToolKind::Lsp => set.lsp = true,
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
            ToolKind::Card => self.card,
            ToolKind::Pipeline => self.pipeline,
            ToolKind::Routine => self.routine,
            ToolKind::Coding => self.coding,
            ToolKind::Math => self.math,
            ToolKind::Git => self.git,
            ToolKind::Lsp => self.lsp,
        }
    }

    /// Any kanban-family permission (board, card, pipeline, routine).
    pub const fn any_board(&self) -> bool {
        self.board || self.card || self.pipeline || self.routine
    }

    /// Copy without the coding tool (used when the work tree has no git
    /// repo, so coding writes would have nothing to belong to).
    pub const fn without_coding(&self) -> Self {
        Self {
            coding: false,
            ..*self
        }
    }
}
