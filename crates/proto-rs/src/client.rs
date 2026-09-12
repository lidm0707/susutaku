use tokio::net::TcpStream;

use crate::codec::{read_frame, write_frame};
use crate::envelope::{ClientMeta, Envelope, Kind};

/// Connects to a hub, announces itself with [`Kind::Register`] carrying the
/// mandatory [`ClientMeta`], then serves inbound [`Kind::Command`] and
/// [`Kind::AgentNames`] envelopes through `on_command` / `on_agents` until
/// the connection drops. Fails before connecting when `meta` is incomplete —
/// the hub would refuse the registration anyway.
pub async fn connect(
    addr: &str,
    meta: ClientMeta,
    mut on_command: impl FnMut(&str, &str) -> String,
    mut on_agents: impl FnMut() -> Vec<crate::envelope::AgentBrief>,
) -> Result<(), String> {
    if !meta.is_valid() {
        return Err(
            "incomplete client metadata (need hostname, os, arch, role model|worker, ram_gib)"
                .to_string(),
        );
    }
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("connect to {addr}: {e}"))?;
    let (mut rd, mut wr) = stream.into_split();
    let register = Envelope {
        id: FIRST_ENVELOPE_ID,
        kind: Kind::Register {
            hostname: meta.hostname,
            os: meta.os,
            arch: meta.arch,
            role: meta.role,
            ram_gib: meta.ram_gib,
        },
    };
    write_frame(&mut wr, &register).await?;
    loop {
        let env = read_frame(&mut rd).await?;
        match env.kind {
            Kind::Command { cmd, agent } => {
                let output = on_command(&agent, &cmd);
                let reply = Envelope {
                    id: env.id,
                    kind: Kind::Result { output },
                };
                write_frame(&mut wr, &reply).await?;
            }
            Kind::AgentNames => {
                let reply = Envelope {
                    id: env.id,
                    kind: Kind::AgentNamesResult {
                        agents: on_agents(),
                    },
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
            Kind::AgentNamesResult { .. } => {
                return Err("unexpected AgentNamesResult from server".to_string());
            }
        }
    }
}

pub const FIRST_ENVELOPE_ID: u64 = 1;
