//! Error type: one enum for every git operation failure.

use std::fmt;

#[derive(Debug)]
pub enum GitError {
    Git(String),
    Io(std::io::Error),
    NoHead,
    Utf8,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git(m) => write!(f, "git: {m}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::NoHead => write!(f, "repository has no commits yet"),
            Self::Utf8 => write!(f, "non-utf8 git data"),
        }
    }
}

impl std::error::Error for GitError {}

impl From<git2::Error> for GitError {
    fn from(e: git2::Error) -> Self {
        Self::Git(e.message().to_string())
    }
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
