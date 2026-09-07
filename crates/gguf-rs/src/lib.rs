//! GGUF inference engine (candle CPU backend). Same call shape as the MLX
//! model so the backend engine thread can run either interchangeably.

pub mod model;
pub mod sample;
pub mod weights;

pub use model::Model;

pub const GGUF_EXT: &str = "gguf";
pub const TEMP: f64 = 0.7;
pub const EOS_CANDIDATES: [&str; 6] = [
    "<|im_end|>",
    "<|eot_id|>",
    "</s>",
    "<|end_of_turn|>",
    "<|endoftext|>",
    "<eos>",
];
