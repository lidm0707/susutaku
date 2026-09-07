use crate::section::{Role, Section};
use crate::{Prompt, MAX_PROMPT_CHARS};

#[derive(Debug)]
pub enum PromptError {
    TooLarge { len: usize, max: usize },
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::TooLarge { len, max } => {
                write!(f, "prompt is {len} chars, exceeds max of {max}")
            }
        }
    }
}

impl std::error::Error for PromptError {}

#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    parts: Vec<Section>,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn section(mut self, role: Role, body: impl Into<String>) -> Self {
        self.parts.push(Section::new(role, body));
        self
    }

    pub fn system(self, body: impl Into<String>) -> Self {
        self.section(Role::System, body)
    }

    pub fn instruction(self, body: impl Into<String>) -> Self {
        self.section(Role::Instruction, body)
    }

    pub fn context(self, body: impl Into<String>) -> Self {
        self.section(Role::Context, body)
    }

    pub fn tool(self, body: impl Into<String>) -> Self {
        self.section(Role::Tool, body)
    }

    pub fn len(&self) -> usize {
        self.parts.iter().map(Section::char_len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn build(self) -> Result<Prompt, PromptError> {
        let len = self.len();
        if len > MAX_PROMPT_CHARS {
            return Err(PromptError::TooLarge {
                len,
                max: MAX_PROMPT_CHARS,
            });
        }
        Ok(Prompt::from_parts(self.parts))
    }
}
