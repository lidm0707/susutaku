//! Zai cloud engine: adapts the Z.ai HTTP client to the Inference port so the
//! chat tool loop can run on a cloud model.

use std::sync::Arc;

use ai_interface_layer::message::Message;
use ai_interface_layer::provider::ChatProvider;
use ai_interface_layer::request::ChatRequest;
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;
use zai_api::client::DEFAULT_MODEL;

use crate::domain::{GenReply, ReplyRx};
use crate::infra::zai_settings::SettingsState;
use crate::port::outbound::Inference;

pub struct ZaiEngine {
    settings: Arc<SettingsState>,
    model: Option<String>,
}

impl ZaiEngine {
    pub fn new(settings: Arc<SettingsState>, model: Option<String>) -> Self {
        Self { settings, model }
    }

    fn client(&self) -> Result<zai_api::client::ZaiClient, String> {
        match &self.model {
            Some(m) => Ok(zai_api::client::ZaiClient::from_key(
                &self.settings.zai_token()?,
                m,
            )),
            None => self.settings.zai_client(),
        }
    }

    fn name(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string())
    }
}

impl Inference for ZaiEngine {
    fn submit(
        &self,
        prompt: String,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<ReplyRx, String> {
        let client = self.client()?;
        let model = self.name();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let req = ChatRequest::new(String::new(), vec![Message::user(prompt)]);
            let reply = ChatProvider::complete(&client, &req).map(|r| GenReply {
                model,
                text: r.content,
                stats: GenStats::default(),
            });
            let _ = tx.send(reply.map_err(|e| e.to_string()));
        });
        Ok(rx)
    }
}
