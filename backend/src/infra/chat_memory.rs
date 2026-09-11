//! Qdrant-backed chat memory adapter: OpenAI-compatible embeddings endpoint +
//! Qdrant REST. Disabled when `SUSUTAKU_EMBEDDINGS_URL` is unset.

use crate::domain::MemoryHit;
use crate::port::outbound::ChatMemory;

pub const EMBEDDINGS_URL_ENV: &str = "SUSUTAKU_EMBEDDINGS_URL";
pub const EMBEDDINGS_MODEL_ENV: &str = "SUSUTAKU_EMBEDDINGS_MODEL";
const EMBEDDINGS_MODEL_DEFAULT: &str = "nomic-embed-text";
pub const QDRANT_URL_ENV: &str = "QDRANT_URL";
const QDRANT_URL_DEFAULT: &str = "http://127.0.0.1:6333";
pub const QDRANT_COLLECTION_ENV: &str = "QDRANT_COLLECTION";
const QDRANT_COLLECTION_DEFAULT: &str = "chat_memory";
const QDRANT_API_KEY_ENV: &str = "QDRANT_API_KEY";

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Builds the memory when the embeddings endpoint is configured; `None` means
/// chat runs without long-term memory.
pub fn from_env() -> Option<QdrantMemory> {
    let url = std::env::var(EMBEDDINGS_URL_ENV)
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(QdrantMemory {
        embeddings_url: url,
        embeddings_model: env_or(EMBEDDINGS_MODEL_ENV, EMBEDDINGS_MODEL_DEFAULT),
        qdrant_url: env_or(QDRANT_URL_ENV, QDRANT_URL_DEFAULT),
        collection: env_or(QDRANT_COLLECTION_ENV, QDRANT_COLLECTION_DEFAULT),
        api_key: std::env::var(QDRANT_API_KEY_ENV).ok(),
    })
}

pub struct QdrantMemory {
    embeddings_url: String,
    embeddings_model: String,
    qdrant_url: String,
    collection: String,
    api_key: Option<String>,
}

impl QdrantMemory {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let body = serde_json::json!({ "input": text, "model": self.embeddings_model });
        let mut req = ureq::post(&self.embeddings_url).set("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("Authorization", &format!("Bearer {key}"));
        }
        let resp = req
            .send_string(&body.to_string())
            .map_err(|e| e.to_string())?;
        let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        let vec = value["data"][0]["embedding"]
            .as_array()
            .ok_or("embeddings reply missing data[0].embedding")?;
        vec.iter()
            .map(|v| {
                v.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| "bad embedding value".to_string())
            })
            .collect()
    }

    fn with_auth(&self, req: ureq::Request) -> ureq::Request {
        match &self.api_key {
            Some(key) => req.set("api-key", key),
            None => req,
        }
    }

    fn ensure_collection(&self, dim: usize) -> Result<(), String> {
        let url = format!("{}/collections/{}", self.qdrant_url, self.collection);
        let resp = self.with_auth(ureq::get(&url)).call();
        if matches!(&resp, Ok(r) if r.status() == 200) {
            return Ok(());
        }
        let body = serde_json::json!({ "vectors": { "size": dim, "distance": "Cosine" } });
        self.with_auth(ureq::put(&url))
            .send_string(&body.to_string())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn upsert(&self, role: &str, text: &str, vector: Vec<f32>) -> Result<(), String> {
        let point = serde_json::json!({
            "points": [{
                "id": uuid_v4_counter(),
                "vector": vector,
                "payload": { "role": role, "text": text },
            }]
        });
        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.qdrant_url, self.collection
        );
        self.with_auth(ureq::post(&url))
            .send_string(&point.to_string())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Point ids need only be unique per collection; a process-wide counter is
/// enough here and avoids a uuid dependency.
fn uuid_v4_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    (ts << 8) | (NEXT.fetch_add(1, Ordering::Relaxed) & 0xff)
}

impl ChatMemory for QdrantMemory {
    fn remember(&self, role: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let vector = self.embed(text)?;
        let dim = vector.len();
        self.ensure_collection(dim)?;
        self.upsert(role, text, vector)
    }

    fn recall(&self, query: &str, k: usize) -> Result<Vec<MemoryHit>, String> {
        let vector = self.embed(query)?;
        let body = serde_json::json!({ "vector": vector, "limit": k, "with_payload": true });
        let url = format!(
            "{}/collections/{}/points/search",
            self.qdrant_url, self.collection
        );
        let resp = self
            .with_auth(ureq::post(&url))
            .send_string(&body.to_string())
            .map_err(|e| e.to_string())?;
        let value: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        Ok(value["result"]
            .as_array()
            .map(|hits| {
                hits.iter()
                    .filter_map(|h| {
                        Some(MemoryHit {
                            role: h["payload"]["role"].as_str().unwrap_or("user").to_string(),
                            text: h["payload"]["text"].as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}
