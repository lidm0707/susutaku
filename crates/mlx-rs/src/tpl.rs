//! Chat prompt templates. Portable: shared by the MLX and GGUF engines.

/// Chat prompt wrapper, selected by the loaded model's architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTpl {
    /// Qwen ChatML: `<|im_start|>user ... <|im_end|><|im_start|>assistant`
    Chatml,
    /// Gemma turn format with a `<|think|>` system turn.
    Turn,
}

impl ChatTpl {
    /// Wrap a raw prompt body so the model answers it. `think` toggles the
    /// reasoning block (gemma system `<|think|>` turn / qwen prefilled empty
    /// think block).
    pub fn wrap(self, body: &str, think: bool) -> String {
        match self {
            Self::Chatml => {
                let opener = if think {
                    "<|im_start|>assistant\n"
                } else {
                    "<|im_start|>assistant\n<think>\n\n</think>\n"
                };
                format!("<|im_start|>user\n{body}<|im_end|>\n{opener}")
            }
            Self::Turn => {
                let system = if think {
                    "<|turn>system\n<|think|>\n<turn|>\n"
                } else {
                    ""
                };
                format!("{system}<|turn>user\n{body}<turn|>\n<|turn>model\n")
            }
        }
    }
}
