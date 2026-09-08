use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::envelope::Envelope;
use crate::{LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES};

pub async fn read_frame<R: AsyncReadExt + Unpin>(rd: &mut R) -> Result<Envelope, String> {
    let mut len_buf = [0u8; LENGTH_PREFIX_BYTES];
    rd.read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read frame length: {e}"))?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(format!("frame of {len} bytes exceeds {MAX_FRAME_BYTES}"));
    }
    let mut buf = vec![0u8; len as usize];
    rd.read_exact(&mut buf)
        .await
        .map_err(|e| format!("read frame body: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("decode envelope: {e}"))
}

pub async fn write_frame<W: AsyncWriteExt + Unpin>(wr: &mut W, env: &Envelope) -> Result<(), String> {
    let body = serde_json::to_vec(env).map_err(|e| format!("encode envelope: {e}"))?;
    let len = (body.len() as u32).to_le_bytes();
    wr.write_all(&len)
        .await
        .map_err(|e| format!("write frame length: {e}"))?;
    wr.write_all(&body)
        .await
        .map_err(|e| format!("write frame body: {e}"))?;
    wr.flush().await.map_err(|e| format!("flush frame: {e}"))
}
