mod builder;
mod section;

pub use builder::{PromptBuilder, PromptError};
pub use section::{Role, Section};

pub const MAX_PROMPT_CHARS: usize = 128 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    parts: Vec<Section>,
}

impl Prompt {
    pub(crate) fn from_parts(parts: Vec<Section>) -> Self {
        Self { parts }
    }

    pub fn parts(&self) -> &[Section] {
        &self.parts
    }

    pub fn render(&self) -> String {
        let mut out = String::with_capacity(self.len());
        for part in &self.parts {
            part.write_into(&mut out);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.parts.iter().map(Section::char_len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

impl std::fmt::Display for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}
