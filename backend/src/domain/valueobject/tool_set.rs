//! Tool permission set: which tools one chat agent may use.

/// Tools an agent can be granted. Order matters only for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Search,
    Fetch,
    Shell,
    Board,
    Card,
    Routine,
    Coding,
    Math,
    Git,
    Lsp,
    Agent,
}

pub const TOOL_KIND_NAMES: [(&str, ToolKind); 11] = [
    ("search", ToolKind::Search),
    ("fetch", ToolKind::Fetch),
    ("shell", ToolKind::Shell),
    ("board", ToolKind::Board),
    ("card", ToolKind::Card),
    ("routine", ToolKind::Routine),
    ("coding", ToolKind::Coding),
    ("math", ToolKind::Math),
    ("git", ToolKind::Git),
    ("lsp", ToolKind::Lsp),
    ("agent", ToolKind::Agent),
];

/// Legacy allow-list entry from the removed pipeline feature; parsed but
/// grants nothing so old agent rows keep loading.
const LEGACY_TOOL_NAMES: [&str; 1] = ["pipeline"];

/// Allow-list of tools for one agent. An empty allow-list means every tool
/// is permitted; a non-empty list permits only the listed kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolSet {
    pub search: bool,
    pub fetch: bool,
    pub shell: bool,
    pub board: bool,
    pub card: bool,
    pub routine: bool,
    pub coding: bool,
    pub math: bool,
    pub git: bool,
    pub lsp: bool,
    pub agent: bool,
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
            routine: true,
            coding: true,
            math: true,
            git: true,
            lsp: true,
            agent: true,
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
            if LEGACY_TOOL_NAMES.contains(&name) {
                continue;
            }
            let (_, kind) = TOOL_KIND_NAMES
                .iter()
                .find(|(n, _)| *n == name)
                .ok_or_else(|| {
                    format!(
                        "unknown tool {name:?} (use search|fetch|shell|board|card|routine|coding|math|git|lsp|agent)"
                    )
                })?;
            match kind {
                ToolKind::Search => set.search = true,
                ToolKind::Fetch => set.fetch = true,
                ToolKind::Shell => set.shell = true,
                ToolKind::Board => set.board = true,
                ToolKind::Card => set.card = true,
                ToolKind::Routine => set.routine = true,
                ToolKind::Coding => set.coding = true,
                ToolKind::Math => set.math = true,
                ToolKind::Git => set.git = true,
                ToolKind::Lsp => set.lsp = true,
                ToolKind::Agent => set.agent = true,
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
            ToolKind::Routine => self.routine,
            ToolKind::Coding => self.coding,
            ToolKind::Math => self.math,
            ToolKind::Git => self.git,
            ToolKind::Lsp => self.lsp,
            ToolKind::Agent => self.agent,
        }
    }

    /// Any task-family permission (board, card, routine).
    pub const fn any_board(&self) -> bool {
        self.board || self.card || self.routine
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
