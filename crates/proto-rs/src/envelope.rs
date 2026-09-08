use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Kind {
    Register { hostname: String, os: String },
    Command { cmd: String },
    Result { output: String },
    Heartbeat,
}
