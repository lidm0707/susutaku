use serde_json::Value;

/// LSP DiagnosticSeverity (1–4), unknown values fall back to Information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    pub fn from_num(n: u64) -> Self {
        match n {
            1 => Self::Error,
            2 => Self::Warning,
            3 => Self::Information,
            _ => Self::Hint,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Information => "info",
            Self::Hint => "hint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn parse(v: &Value) -> Option<Self> {
        let range = &v["range"];
        Some(Self {
            line: range["start"]["line"].as_u64()? as u32,
            col_start: range["start"]["character"].as_u64()? as u32,
            col_end: range["end"]["character"].as_u64()? as u32,
            severity: Severity::from_num(v["severity"].as_u64().unwrap_or(3)),
            message: v["message"].as_str()?.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
}

impl Location {
    pub fn parse(v: &Value) -> Option<Self> {
        let range = &v["range"];
        Some(Self {
            uri: v["uri"].as_str()?.to_string(),
            line: range["start"]["line"].as_u64()? as u32,
            col_start: range["start"]["character"].as_u64()? as u32,
            col_end: range["end"]["character"].as_u64()? as u32,
        })
    }

    pub fn render(&self) -> String {
        format!("{}:{}:{}", self.uri, self.line + 1, self.col_start + 1)
    }
}

/// Convert a UTF-8 (line, column) pair into the LSP UTF-16 character offset.
pub fn utf16_col(line_text: &str, col_utf8: usize) -> u32 {
    line_text
        .char_indices()
        .take_while(|(i, _)| *i < col_utf8)
        .map(|(_, c)| c.len_utf16() as u32)
        .sum()
}

/// Extract the full text of a 0-based line without allocating the whole file twice.
pub fn line_text(text: &str, line: u32) -> Option<&str> {
    text.lines().nth(line as usize)
}
