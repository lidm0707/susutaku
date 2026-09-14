//! Per-tool skill sheets: short playbooks embedded at compile time and
//! injected into the model context whenever the matching tool is enabled.

pub const SEARCH: &str = include_str!("../skills/search.md");
pub const FETCH: &str = include_str!("../skills/fetch.md");
pub const SHELL: &str = include_str!("../skills/shell.md");
pub const BOARD: &str = include_str!("../skills/board.md");
pub const CODING: &str = include_str!("../skills/coding.md");
pub const MATH: &str = include_str!("../skills/math.md");
pub const GIT: &str = include_str!("../skills/git.md");
pub const LSP: &str = include_str!("../skills/lsp.md");

/// Tool name → skill sheet. Names match the backend `TOOL_KIND_NAMES`.
const TOOL_SKILLS: [(&str, &str); 8] = [
    ("search", SEARCH),
    ("fetch", FETCH),
    ("shell", SHELL),
    ("board", BOARD),
    ("coding", CODING),
    ("math", MATH),
    ("git", GIT),
    ("lsp", LSP),
];

pub const TOOL_NAMES: [&str; 11] = [
    "search", "fetch", "shell", "board", "card", "pipeline", "routine", "coding", "math", "git",
    "lsp",
];

pub fn skill(name: &str) -> Option<&'static str> {
    TOOL_SKILLS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
}
