use std::path::PathBuf;

use backend::domain::{LspOp, ToolCall, ToolKind, ToolSet};
use backend::port::outbound::Runner;

#[test]
fn parses_lsp_tool_line() {
    assert_eq!(
        ToolCall::parse("TOOL: LSP definition src/lib.rs 10 5"),
        Some(ToolCall::Lsp {
            op: LspOp::Definition,
            path: "src/lib.rs".into(),
            line: 10,
            col: 5,
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: LSP REFERENCES crates/core-agent/src/lib.rs 0 0"),
        Some(ToolCall::Lsp {
            op: LspOp::References,
            path: "crates/core-agent/src/lib.rs".into(),
            line: 0,
            col: 0,
        })
    );
    assert_eq!(
        ToolCall::parse("TOOL: LSP hover src/main.rs 3 12"),
        Some(ToolCall::Lsp {
            op: LspOp::Hover,
            path: "src/main.rs".into(),
            line: 3,
            col: 12,
        })
    );
}

#[test]
fn rejects_malformed_lsp_tool_line() {
    assert_eq!(ToolCall::parse("TOOL: LSP"), None);
    assert_eq!(ToolCall::parse("TOOL: LSP definition"), None);
    assert_eq!(ToolCall::parse("TOOL: LSP definition src/lib.rs 10"), None);
    assert_eq!(ToolCall::parse("TOOL: LSP rename src/lib.rs 10 5"), None);
    assert_eq!(
        ToolCall::parse("TOOL: LSP definition src/lib.rs ten 5"),
        None
    );
}

#[test]
fn parses_lsp_xml_invoke() {
    assert_eq!(
        ToolCall::parse(
            r#"<invoke name="lsp"><parameter name="op">definition</parameter><parameter name="path">src/lib.rs</parameter><parameter name="line">10</parameter><parameter name="col">5</parameter></invoke>"#
        ),
        Some(ToolCall::Lsp {
            op: LspOp::Definition,
            path: "src/lib.rs".into(),
            line: 10,
            col: 5,
        })
    );
}

#[test]
fn lsp_tool_permission() {
    assert!(ToolSet::all().allows(ToolKind::Lsp));
    let set = ToolSet::from_names(&["lsp".to_string()]).unwrap();
    assert!(set.allows(ToolKind::Lsp));
    assert!(!set.allows(ToolKind::Shell));
    assert!(!set.allows(ToolKind::Git));
}

struct WorkTree(PathBuf);

impl Runner for WorkTree {
    fn run(&self, _cmd: &str) -> Result<String, String> {
        Err("no shell in test".to_string())
    }
    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let target = self.path(path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(target, content).map_err(|e| e.to_string())
    }
    fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.path(path)?).map_err(|e| e.to_string())
    }
    fn workspace_root(&self) -> PathBuf {
        self.0.clone()
    }
    fn has_git_repo(&self) -> bool {
        false
    }
    fn git(&self, _op: &backend::domain::GitOp) -> Result<String, String> {
        Err("no work tree in test".to_string())
    }
}

impl WorkTree {
    /// Mirrors the sandbox adapter: absolute paths root at the tree, `..`
    /// escapes are rejected.
    fn path(&self, path: &str) -> Result<PathBuf, String> {
        let trimmed = path.trim().trim_start_matches('/');
        let cleaned: Vec<&str> = trimmed
            .split('/')
            .filter(|seg| !seg.is_empty() && *seg != ".")
            .collect();
        if cleaned.contains(&"..") {
            return Err(format!("coding path escapes the work tree: {path}"));
        }
        if cleaned.is_empty() {
            return Err("coding path is empty".to_string());
        }
        Ok(self.0.join(cleaned.join("/")))
    }
}

#[test]
fn runner_read_file_round_trip() {
    let dir = std::env::temp_dir().join("susutaku-lsp-tool-test");
    std::fs::create_dir_all(&dir).unwrap();
    let runner = WorkTree(dir.clone());
    runner.write_file("src/a.rs", "fn main() {}\n").unwrap();
    assert_eq!(runner.read_file("src/a.rs").unwrap(), "fn main() {}\n");
    assert!(runner.read_file("../etc/passwd").is_err());
    assert_eq!(runner.workspace_root(), dir);
    std::fs::remove_dir_all(&dir).unwrap();
}
