pub const NEWLINE: char = '\n';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    NotFound(String),
    Parse(String),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::NotFound(p) => write!(f, "pdf not found: {p}"),
            ExtractError::Parse(e) => write!(f, "pdf parse error: {e}"),
        }
    }
}

impl std::error::Error for ExtractError {}

pub fn extract_text(path: &std::path::Path) -> Result<String, ExtractError> {
    extract_text_opt(path, None)
}

pub fn extract_text_encrypted(path: &std::path::Path, password: &str) -> Result<String, ExtractError> {
    extract_text_opt(path, Some(password))
}

fn extract_text_opt(path: &std::path::Path, password: Option<&str>) -> Result<String, ExtractError> {
    if !path.is_file() {
        return Err(ExtractError::NotFound(path.display().to_string()));
    }
    match password {
        Some(pw) => pdf_extract::extract_text_encrypted(path, pw),
        None => pdf_extract::extract_text(path),
    }
    .map_err(|e| ExtractError::Parse(e.to_string()))
}

pub fn extract_lines(path: &std::path::Path) -> Result<Vec<String>, ExtractError> {
    extract_lines_encrypted(path, None)
}

pub fn extract_lines_encrypted(path: &std::path::Path, password: Option<&str>) -> Result<Vec<String>, ExtractError> {
    let text = match password {
        Some(pw) => extract_text_encrypted(path, pw)?,
        None => extract_text(path)?,
    };
    Ok(text
        .split(NEWLINE)
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}
