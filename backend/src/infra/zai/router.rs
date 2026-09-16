//! Routes an agent's configured model name to a cloud engine: a model known
//! to the Z.ai settings resolves to a [`ZaiEngine`]; anything else (local
//! model names, e2e mock names) resolves to `None` so the caller uses the
//! shared engine.

use std::sync::Arc;

use crate::port::outbound::{Inference, ModelEngines};

use super::chat::ZaiEngine;
use super::settings::SettingsState;

pub struct ZaiRouter {
    settings: Arc<SettingsState>,
}

impl ZaiRouter {
    pub fn new(settings: Arc<SettingsState>) -> Self {
        Self { settings }
    }
}

impl ModelEngines for ZaiRouter {
    fn engine_for(&self, model: &str) -> Option<Arc<dyn Inference>> {
        let name = model.trim();
        if name.is_empty() {
            return None;
        }
        let zai = self.settings.zai();
        let known =
            zai.model.as_deref() == Some(name) || zai.models.iter().any(|m| m.model == name);
        if !known {
            return None;
        }
        Some(Arc::new(ZaiEngine::new(
            self.settings.clone(),
            Some(name.to_owned()),
        )))
    }
}
