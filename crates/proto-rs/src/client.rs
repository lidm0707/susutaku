use tokio::net::TcpStream;

use crate::codec::{read_frame, write_frame};
use crate::envelope::{Envelope, Kind};

/// Connects to a hub, announces itself with [`Kind::Register`], then serves
/// inbound [`Kind::Command`] envelopes through `on_command` until the
/// connection drops.
pub async fn connect(
    addr: &str,
    hostname: String,
    os: String,
    mut on_command: impl FnMut(&str) -> String,
) -> Result<(), String> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("connect to {addr}: {e}"))?;
    let (mut rd, mut wr) = stream.into_split();
    let register = Envelope {
        id: FIRST_ENVELOPE_ID,
        kind: Kind::Register { hostname, os },
    };
    write_frame(&mut wr, &register).await?;
    loop {
        let env = read_frame(&mut rd).await?;
        match env.kind {
            Kind::Command { cmd } => {
                let output = on_command(&cmd);
                let reply = Envelope {
                    id: env.id,
                    kind: Kind::Result { output },
                };
                write_frame(&mut wr, &reply).await?;
            }
            Kind::Heartbeat => {
                let reply = Envelope {
                    id: env.id,
                    kind: Kind::Heartbeat,
                };
                write_frame(&mut wr, &reply).await?;
            }
            Kind::Register { .. } => {
                return Err("unexpected Register from server".to_string());
            }
            Kind::Result { .. } => {
                return Err("unexpected Result from server".to_string());
            }
        }
    }
}

pub const FIRST_ENVELOPE_ID: u64 = 1;
