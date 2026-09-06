//! Dedicated inference thread owning the non-Send MLX model.
//! The rest of the app talks to it through the clonable [`Engine`] handle.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, RwLock};

use crate::port::outbound::{GenReply, Inference, ModelSwitch, ReplyRx};
use susutaku_mlx::engine::Model;
use susutaku_mlx::tok::{ChatTok, TokKind};

pub const MODELS_ROOT: &str = "models";

/// Thread-safe handle to the inference thread.
#[derive(Clone)]
pub struct Engine {
    name: Arc<String>,
    tx: Sender<Job>,
}

struct Job {
    prompt: String,
    max_tokens: usize,
    tok: TokKind,
    think: bool,
    reply: tokio::sync::oneshot::Sender<Result<GenReply, String>>,
}

impl Engine {
    /// Spawn the engine on the first runnable model under [`MODELS_ROOT`].
    pub fn spawn_first() -> Result<Engine, String> {
        let models = hf_loader::loadable_models(&PathBuf::from(MODELS_ROOT));
        let entry = models
            .iter()
            .find(|e| Model::supported(&e.path))
            .or_else(|| models.first())
            .ok_or_else(|| format!("no loadable MLX 4bit model under {MODELS_ROOT}"))?;
        Ok(Self::spawn(entry.path.clone()))
    }

    pub fn spawn_by_name(name: &str) -> Result<Engine, String> {
        let entry = hf_loader::loadable_models(&PathBuf::from(MODELS_ROOT))
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("no loadable model named `{name}` under {MODELS_ROOT}"))?;
        Ok(Self::spawn(entry.path))
    }

    fn spawn(model_dir: PathBuf) -> Engine {
        let name = model_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = channel::<Job>();
        std::thread::spawn(move || run(model_dir, rx));
        Engine {
            name: Arc::new(name),
            tx,
        }
    }

    fn model_name(&self) -> &str {
        &self.name
    }
}

impl Inference for Engine {
    /// Queue one generation; the result arrives on the returned receiver.
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Job {
                prompt,
                max_tokens,
                tok,
                think,
                reply: tx,
            })
            .map_err(|_| "inference thread stopped".to_string())?;
        Ok(rx)
    }
}

fn fail_all(rx: &Receiver<Job>, err: String) {
    for job in rx.iter() {
        let _ = job.reply.send(Err(err.clone()));
    }
}

/// Holds the active engine; [`select`](ModelSwitch::select) swaps it without restart.
#[derive(Clone)]
pub struct ModelPool {
    active: Arc<RwLock<Engine>>,
}

impl ModelPool {
    pub fn spawn_first() -> Result<ModelPool, String> {
        Ok(ModelPool {
            active: Arc::new(RwLock::new(Engine::spawn_first()?)),
        })
    }
}

fn run(model_dir: PathBuf, rx: Receiver<Job>) {
    let name = model_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(mut model) = Model::load(&model_dir) else {
        fail_all(&rx, format!("failed to load model {name}"));
        return;
    };
    // Both tokenizer variants are built once per model thread; a katgpt
    // build failure only surfaces when a job actually requests it.
    let normal = ChatTok::load(&model_dir, TokKind::Normal)
        .map_err(|e| format!("failed to read tokenizer in {name}: {e}"));
    let katgpt = ChatTok::load(&model_dir, TokKind::Katgpt)
        .map_err(|e| format!("failed to build katgpt tokenizer in {name}: {e}"));
    while let Ok(job) = rx.recv() {
        let picked = match job.tok {
            TokKind::Normal => normal.as_ref().map_err(|e| e.clone()),
            TokKind::Katgpt => katgpt.as_ref().map_err(|e| e.clone()),
        };
        let result = picked.and_then(|tokenizer| {
            let prompt = model.chat_tpl().wrap(&job.prompt, job.think);
            model
                .chat_stats(tokenizer, &prompt, job.max_tokens)
                .map(|(text, stats)| GenReply {
                    model: name.clone(),
                    text,
                    stats,
                })
                .map_err(|e| e.to_string())
        });
        let _ = job.reply.send(result);
    }
}

impl Inference for ModelPool {
    fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        tok: TokKind,
        think: bool,
    ) -> Result<ReplyRx, String> {
        self.active
            .read()
            .map_err(|_| "model state poisoned".to_string())?
            .submit(prompt, max_tokens, tok, think)
    }
}

impl ModelSwitch for ModelPool {
    fn select(&self, name: &str) -> Result<(), String> {
        let next = Engine::spawn_by_name(name)?;
        match self.active.write() {
            Ok(mut slot) => *slot = next,
            Err(_) => return Err("model state poisoned".to_string()),
        }
        Ok(())
    }

    fn selected(&self) -> Option<String> {
        Some(self.active.read().ok()?.model_name().to_string())
    }
}
