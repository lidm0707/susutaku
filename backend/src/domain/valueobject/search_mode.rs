//! Value object: how search is engaged. The model routes itself (Auto),
//! always (Force), never (Off).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Off,
    Auto,
    Force,
}

impl SearchMode {
    /// "on" = always search, "off" = never, absent/"auto" = model routes itself.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("on") => Self::Force,
            Some("off") => Self::Off,
            _ => Self::Auto,
        }
    }

    /// Wrap a message so the model answers it. The architecture-specific
    /// chat template is applied by the inference engine (it knows the arch);
    /// this only frames the body as a single user turn.
    pub fn wrap(message: &str) -> String {
        message.to_string()
    }
}
