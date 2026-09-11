use std::process::Command;

use crate::client::LspClient;
use crate::types::line_text;
use crate::Location;

pub const DEFAULT_LANGUAGE: &str = "rust";

/// One-shot session: spawn the server, open `path`, run `query`, shut down.
/// `line`/`col` are 0-based UTF-8 positions, converted to LSP UTF-16.
fn session<T>(
    cmd: &mut Command,
    root: &str,
    path: &str,
    text: &str,
    line: u32,
    col_utf8: usize,
    query: impl FnOnce(&mut LspClient, &str, [u32; 2]) -> Result<T, std::io::Error>,
) -> Result<T, String> {
    let mut client = LspClient::spawn(cmd, root).map_err(|e| e.to_string())?;
    let uri = uri_of(path);
    let opened = client
        .open(&uri, DEFAULT_LANGUAGE, text)
        .map_err(|e| e.to_string());
    if let Err(e) = opened {
        let _ = client.shutdown();
        return Err(e);
    }
    let col = line_text(text, line)
        .map(|l| crate::types::utf16_col(l, col_utf8))
        .unwrap_or(0);
    let out = query(&mut client, &uri, [line, col]).map_err(|e| e.to_string());
    let _ = client.shutdown();
    out
}

pub fn uri_of(path: &str) -> String {
    format!("file://{path}")
}

pub fn goto_definition(
    cmd: &mut Command,
    root: &str,
    path: &str,
    text: &str,
    line: u32,
    col: usize,
) -> Result<Vec<Location>, String> {
    session(cmd, root, path, text, line, col, |c, uri, pos| {
        c.definition(uri, pos)
    })
}

pub fn references(
    cmd: &mut Command,
    root: &str,
    path: &str,
    text: &str,
    line: u32,
    col: usize,
) -> Result<Vec<Location>, String> {
    session(cmd, root, path, text, line, col, |c, uri, pos| {
        c.references(uri, pos, true)
    })
}

pub fn hover(
    cmd: &mut Command,
    root: &str,
    path: &str,
    text: &str,
    line: u32,
    col: usize,
) -> Result<Option<String>, String> {
    session(cmd, root, path, text, line, col, |c, uri, pos| {
        c.hover(uri, pos)
    })
}
