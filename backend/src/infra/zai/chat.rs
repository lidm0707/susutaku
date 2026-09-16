//! Zai cloud engine: adapts the Z.ai HTTP client to the Inference port so the
//! chat tool loop can run on a cloud model.

use std::sync::Arc;

use ai_interface_layer::message::Message;
use ai_interface_layer::provider::ChatProvider;
use ai_interface_layer::request::ChatRequest;
use susutaku_mlx::stats::GenStats;
use susutaku_mlx::tok::TokKind;
use tokio::sync::broadcast;
use zai_api::client::DEFAULT_MODEL;

use crate::domain::{CancelFlag, GenReply, INTERRUPTED_NOTE, ReplyRx};
use crate::infra::zai::settings::SettingsState;
use crate::port::outbound::Inference;

/// Live generation progress for SSE chat: a fresh inference round (`Turn`)
/// resets the client bubble, `Delta` appends one streamed content chunk.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Turn,
    Delta(String),
}

/// Error text returned when a run is cancelled via [`CancelFlag`].
pub const CANCELLED: &str = "interrupted";

/// Capacity of the per-request broadcast of [`StreamEvent`]s; a slow SSE
/// client lags instead of blocking generation.
pub const STREAM_EVENT_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct ZaiEngine {
    settings: Arc<SettingsState>,
    model: Option<String>,
    events: Option<broadcast::Sender<StreamEvent>>,
    cancel: Option<CancelFlag>,
}

impl ZaiEngine {
    pub fn new(settings: Arc<SettingsState>, model: Option<String>) -> Self {
        Self {
            settings,
            model,
            events: None,
            cancel: None,
        }
    }

    /// Enable cooperative cancellation: checked per streamed delta and before
    /// each submit.
    pub fn with_cancel(mut self, cancel: CancelFlag) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Enable streamed generation with progress published on `events`.
    pub fn with_events(mut self, events: broadcast::Sender<StreamEvent>) -> Self {
        self.events = Some(events);
        self
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

    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.is_cancelled())
    }

    fn complete_text(&self, message: Message, model: String) -> Result<GenReply, String> {
        if self.cancelled() {
            return Err(CANCELLED.to_string());
        }
        let Some(events) = &self.events else {
            let client = self.client()?;
            let req = ChatRequest::new(String::new(), vec![message]);
            return ChatProvider::complete(&client, &req)
                .map(|r| GenReply {
                    model,
                    text: r.content,
                    stats: GenStats::default(),
                })
                .map_err(|e| e.to_string());
        };
        let client = self.client()?;
        let req = ChatRequest::new(String::new(), vec![message]);
        let _ = events.send(StreamEvent::Turn);
        match client.complete_stream(&req) {
            Ok(stream) => {
                let mut text = String::new();
                for delta in stream {
                    if self.cancelled() {
                        // keep what was generated so far: the partial reply
                        // reaches the transcript/memory and the next send
                        // continues from it
                        if text.is_empty() {
                            return Err(CANCELLED.to_string());
                        }
                        text.push_str(INTERRUPTED_NOTE);
                        return Ok(GenReply {
                            model,
                            text,
                            stats: GenStats::default(),
                        });
                    }
                    match delta {
                        Ok(d) => {
                            text.push_str(&d);
                            let _ = events.send(StreamEvent::Delta(d));
                        }
                        Err(e) => {
                            if text.is_empty() {
                                break;
                            }
                            return Err(e.to_string());
                        }
                    }
                }
                if text.is_empty() {
                    // stream produced nothing (older gateway, proxy stripping
                    // SSE) — fall back to the one-shot completion
                    let reply = ChatProvider::complete(&client, &req).map_err(|e| e.to_string())?;
                    return Ok(GenReply {
                        model,
                        text: reply.content,
                        stats: GenStats::default(),
                    });
                }
                Ok(GenReply {
                    model,
                    text,
                    stats: GenStats::default(),
                })
            }
            Err(_) => {
                // streaming not accepted by the endpoint — one-shot fallback
                let reply = ChatProvider::complete(&client, &req).map_err(|e| e.to_string())?;
                Ok(GenReply {
                    model,
                    text: reply.content,
                    stats: GenStats::default(),
                })
            }
        }
    }
}

impl Inference for ZaiEngine {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        self.submit_with_image(prompt, None, max_tokens, tok, think)
    }

    fn submit_with_image(
        &self,
        prompt: String,
        image: Option<String>,
        _max_tokens: usize,
        _tok: TokKind,
        _think: bool,
    ) -> Result<ReplyRx, String> {
        if self.cancelled() {
            return Err(CANCELLED.to_string());
        }
        let model = self.name();
        let message = match image {
            Some(url) => Message::user_with_image(prompt, url),
            None => Message::user(prompt),
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let engine = self.clone();
        tokio::task::spawn_blocking(move || {
            let reply = engine.complete_text(message, model);
            let _ = tx.send(reply);
        });
        Ok(rx)
    }
}
