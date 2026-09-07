//! Data flowing through the pipeline.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TEXT_KEY: &str = "text";
pub const SOURCE_KEY: &str = "source";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PayloadKind {
    Text,
    Json,
    Binary,
}

#[derive(Clone, Debug)]
pub struct Payload {
    pub kind: PayloadKind,
    pub data: Vec<u8>,
    pub meta: BTreeMap<String, String>,
}

impl Payload {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            kind: PayloadKind::Text,
            data: content.into().into_bytes(),
            meta: BTreeMap::new(),
        }
    }

    pub fn json(value: serde_json::Value) -> Self {
        let data = serde_json::to_vec(&value).unwrap_or_default();
        Self { kind: PayloadKind::Json, data, meta: BTreeMap::new() }
    }

    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }

    pub fn as_json(&self) -> Option<serde_json::Value> {
        if self.kind != PayloadKind::Json {
            return None;
        }
        serde_json::from_slice(&self.data).ok()
    }

    pub fn set_meta(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.meta.insert(key.into(), value.into());
    }

    pub fn get_meta(&self, key: &str) -> Option<&str> {
        self.meta.get(key).map(String::as_str)
    }
}
