//! solana_wallet — browser wallet keypair + RPC helpers, wasm-ready (gloo).
//!
//! Pure-Rust ed25519 via `ed25519-dalek`, base58 via `bs58`, persistence via
//! `gloo::storage::LocalStorage`, async RPC via `gloo::net::http::Request`.

pub mod keypair;
pub mod rpc;
pub mod store;

pub use keypair::{LAMPORTS_PER_SOL, WalletKeypair};
pub use rpc::{DEVNET_URL, SolanaRpc, devnet};
pub use store::WalletStore;

pub mod js;
