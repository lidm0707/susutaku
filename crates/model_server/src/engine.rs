//! Dedicated inference thread owning the non-Send MLX model.
//! The rest of the server talks to it through the clonable [`Engine`] handle.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, RwLock};

use crate::MODELS_ROOT;
use crate::ports::{GenReply, Inference, ModelSwitch, ReplyRx};
use susutaku_mlx::tok::{ChatTok, TokKind};

/// Which inference backend runs a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Mlx,
    Gguf,
}

impl EngineKind {
    fn of(format: hf_loader::ModelFormat) -> Self {
        match format {
            hf_loader::ModelFormat::Gguf => Self::Gguf,
            _ => Self::Mlx,
        }
    }
}

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
            .find(|e| match e.format {
                hf_loader::ModelFormat::Mlx => susutaku_mlx::engine::Model::supported(&e.path),
                hf_loader::ModelFormat::Gguf => gguf_rs::Model::supported(&e.path),
                hf_loader::ModelFormat::Unknown => false,
            })
            .or_else(|| models.first())
            .ok_or_else(|| format!("no loadable 4bit model under {MODELS_ROOT}"))?;
        Ok(Self::spawn(
            entry.path.clone(),
            EngineKind::of(entry.format.clone()),
        ))
    }

    pub fn spawn_by_name(name: &str) -> Result<Engine, String> {
        let entry = hf_loader::loadable_models(&PathBuf::from(MODELS_ROOT))
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("no loadable model named `{name}` under {MODELS_ROOT}"))?;
        let kind = EngineKind::of(entry.format);
        Ok(Self::spawn(entry.path, kind))
    }

    fn spawn(model_dir: PathBuf, kind: EngineKind) -> Engine {
        let name = model_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = channel::<Job>();
        match kind {
            EngineKind::Gguf => std::thread::spawn(move || run_gguf(model_dir, rx)),
            EngineKind::Mlx => std::thread::spawn(move || run_mlx(model_dir, rx)),
        };
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

/// Shared tokenizer state used by both engine loops.
struct Toks {
    normal: Result<ChatTok, String>,
    katgpt: Result<ChatTok, String>,
}

impl Toks {
    fn load(model_dir: &std::path::Path, name: &str) -> Self {
        Self {
            normal: ChatTok::load(model_dir, TokKind::Normal)
                .map_err(|e| format!("failed to read tokenizer in {name}: {e}")),
            katgpt: ChatTok::load(model_dir, TokKind::Katgpt)
                .map_err(|e| format!("failed to build katgpt tokenizer in {name}: {e}")),
        }
    }

    fn pick(&self, kind: TokKind) -> Result<&ChatTok, String> {
        match kind {
            TokKind::Normal => self.normal.as_ref().map_err(|e| e.clone()),
            TokKind::Katgpt => self.katgpt.as_ref().map_err(|e| e.clone()),
        }
    }
}

/// Stats reply shape shared by both engines.
struct Reply {
    text: String,
    stats: susutaku_mlx::engine::GenStats,
}

fn serve_jobs(
    rx: Receiver<Job>,
    name: &str,
    toks: &Toks,
    mut generate: impl FnMut(&ChatTok, &str, bool, usize) -> Result<Reply, String>,
) {
    while let Ok(job) = rx.recv() {
        let result = toks
            .pick(job.tok)
            .and_then(|tok| generate(tok, &job.prompt, job.think, job.max_tokens))
            .map(|r| GenReply {
                model: name.to_string(),
                text: r.text,
                stats: r.stats,
            });
        let _ = job.reply.send(result);
    }
}

fn fail_all(rx: &Receiver<Job>, err: String) {
    for job in rx.iter() {
        let _ = job.reply.send(Err(err.clone()));
    }
}

/// Holds the active engine; [`ModelSwitch::select`] swaps it without restart.
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

fn run_mlx(model_dir: PathBuf, rx: Receiver<Job>) {
    let name = model_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(mut model) = susutaku_mlx::engine::Model::load(&model_dir) else {
        fail_all(&rx, format!("failed to load model {name}"));
        return;
    };
    // Both tokenizer variants are built once per model thread; a katgpt
    // build failure only surfaces when a job actually requests it.
    let toks = Toks::load(&model_dir, &name);
    let tpl = model.chat_tpl();
    serve_jobs(rx, &name, &toks, |tok, prompt, think, max_tokens| {
        let wrapped = tpl.wrap(prompt, think);
        model
            .chat_stats(tok, &wrapped, max_tokens)
            .map(|(text, stats)| Reply { text, stats })
            .map_err(|e| e.to_string())
    });
}

fn run_gguf(model_dir: PathBuf, rx: Receiver<Job>) {
    let name = model_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(mut model) = gguf_rs::Model::load(&model_dir) else {
        fail_all(&rx, format!("failed to load model {name}"));
        return;
    };
    let toks = Toks::load(&model_dir, &name);
    let tpl = model.chat_tpl();
    serve_jobs(rx, &name, &toks, |tok, prompt, think, max_tokens| {
        let wrapped = tpl.wrap(prompt, think);
        model
            .chat_stats(tok, &wrapped, max_tokens)
            .map(|(text, stats)| Reply { text, stats })
            .map_err(|e| e.to_string())
    });
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
