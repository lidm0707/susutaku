//! Small shared helpers (byte draining, truncation, cwd mapping).

use std::io::Read;

const CONTAINER_WORKSPACE: &str = "/workspace";
const OUTPUT_HEADROOM: usize = 2;

pub(crate) fn drain<R: Read>(rd: &mut R, cap: usize, into: &mut Vec<u8>) {
    let mut buf = [0u8; 8192];
    let mut dropped = false;
    loop {
        match rd.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if dropped {
                    continue;
                }
                into.extend_from_slice(&buf[..n]);
                if into.len() >= cap.saturating_mul(OUTPUT_HEADROOM) {
                    dropped = true;
                }
            }
        }
    }
}

pub(crate) fn truncate_bytes(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
}

/// Map the stored relative cwd to its path inside the container.
pub(crate) fn cwd_mount_path(rel: &str) -> String {
    let rel = rel.trim();
    if rel.is_empty() || rel == "." {
        CONTAINER_WORKSPACE.to_string()
    } else {
        format!("{CONTAINER_WORKSPACE}/{}", rel.trim_start_matches('/'))
    }
}
