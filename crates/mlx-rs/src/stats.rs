//! Generation statistics (pure data; shared by native engine and remote clients).

/// Token counts and wall-clock times of one generation.
#[derive(Debug, Clone, Copy, Default)]
pub struct GenStats {
    pub prompt_tokens: usize,
    pub prompt_secs: f64,
    pub decode_tokens: usize,
    pub decode_secs: f64,
}

impl GenStats {
    pub fn prompt_tps(&self) -> f64 {
        self.prompt_tokens as f64 / self.prompt_secs
    }

    pub fn decode_tps(&self) -> f64 {
        self.decode_tokens as f64 / self.decode_secs
    }
}
