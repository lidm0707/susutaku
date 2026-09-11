use std::process::Command;

pub const DEFAULT_ROOT: &str = ".";
/// Program + args of the language server used by the tool entry points.
pub const DEFAULT_SERVER_CMD: (&str, &[&str]) = ("rust-analyzer", &[]);

fn server() -> Command {
    let (program, args) = DEFAULT_SERVER_CMD;
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd
}

pub fn definition(path: &str, text: &str, line: u32, col: usize) -> Result<String, String> {
    let locations = lsp_rs::goto_definition(&mut server(), DEFAULT_ROOT, path, text, line, col)?;
    Ok(locations
        .iter()
        .map(|l| l.render())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn references(path: &str, text: &str, line: u32, col: usize) -> Result<String, String> {
    let locations = lsp_rs::references(&mut server(), DEFAULT_ROOT, path, text, line, col)?;
    Ok(locations
        .iter()
        .map(|l| l.render())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn hover(path: &str, text: &str, line: u32, col: usize) -> Result<String, String> {
    Ok(lsp_rs::hover(&mut server(), DEFAULT_ROOT, path, text, line, col)?.unwrap_or_default())
}
