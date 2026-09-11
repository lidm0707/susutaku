//! Length-prefixed JSON command protocol over TCP.

pub mod client;
pub mod codec;
pub mod envelope;
pub mod server;

pub use envelope::{ClientMeta, Envelope, Kind, ROLE_MODEL, ROLE_WORKER};

/// Hard cap for one framed payload (16 MiB).
pub const MAX_FRAME_BYTES: u32 = 16 * 1024 * 1024;
/// Little-endian u32 length prefix size.
pub const LENGTH_PREFIX_BYTES: usize = 4;
/// Outbound per-client command queue depth.
pub const OUTBOUND_CHANNEL_CAPACITY: usize = 16;
/// Inbound per-client envelope queue depth.
pub const INBOUND_CHANNEL_CAPACITY: usize = 16;
