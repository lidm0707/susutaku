//! modelless — engine-free language kit: English verb corpus + katgpt BPE.

#[path = "chain-gram/mod.rs"]
pub mod chain_gram;
pub mod corpus;
pub mod engine;
#[path = "sentencepiece/mod.rs"]
pub mod sentencepiece;
