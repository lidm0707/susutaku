#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    Instruction,
    Context,
    Tool,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Role::System),
            "instruction" | "instructions" => Some(Role::Instruction),
            "context" => Some(Role::Context),
            "tool" | "tools" => Some(Role::Tool),
            _ => None,
        }
    }

    pub fn header(self) -> &'static str {
        match self {
            Role::System => "# System",
            Role::Instruction => "# Instructions",
            Role::Context => "# Context",
            Role::Tool => "# Tools",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    role: Role,
    body: String,
}

impl Section {
    pub fn new(role: Role, body: impl Into<String>) -> Self {
        Self {
            role,
            body: body.into(),
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn char_len(&self) -> usize {
        self.role.header().len() + 1 + self.body.len() + 1
    }

    pub fn write_into(&self, out: &mut String) {
        out.push_str(self.role.header());
        out.push('\n');
        out.push_str(&self.body);
        out.push('\n');
    }
}
