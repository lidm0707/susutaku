//! Bounded latent-space context: working memory for chat/agent prompts.
//!
//! Entries are tagged by [`EntryKind`] and kept under a token budget.
//! Eviction uses katgpt-core's `kv_eviction` usage-rate scoring
//! (`cum_mass / max(1, age)`, the arXiv:2608.19920 normalized-H2O rule):
//! admission tick at push, one mass unit observed per touch/render, and
//! [`select_evict_into`] picks the lowest score among unpinned entries —
//! cold-and-old first, ties by ascending index. Pinned entries are never
//! evicted.

use katgpt_core::kv_eviction::{UsageRow, observe, score, select_evict};

pub const DEFAULT_MAX_TOKENS: usize = 8192;
pub const MIN_ENTRIES_KEPT: usize = 1;
const TOUCH_MASS: f32 = 1.0;
const EVICT_BATCH: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    System,
    Focus,
    Memory,
    Thread,
    Tool,
}

impl EntryKind {
    pub fn header(self) -> &'static str {
        match self {
            EntryKind::System => "[system]",
            EntryKind::Focus => "[focus]",
            EntryKind::Memory => "[memory]",
            EntryKind::Thread => "[thread]",
            EntryKind::Tool => "[tool]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
    pub tokens: usize,
    usage: UsageRow,
    pinned: bool,
}

impl Entry {
    pub fn new(kind: EntryKind, text: impl Into<String>, tokens: usize) -> Self {
        Entry {
            kind,
            text: text.into(),
            tokens,
            usage: UsageRow::default(),
            pinned: false,
        }
    }

    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn uses(&self, tick: u64) -> f32 {
        score(&self.usage, tick)
    }
}

#[derive(Debug)]
pub struct Context {
    max_tokens: usize,
    entries: Vec<Entry>,
    tick: u64,
    next_id: u64,
    ids: Vec<Option<u64>>,
}

impl Context {
    pub fn new(max_tokens: usize) -> Self {
        Context {
            max_tokens,
            entries: Vec::new(),
            tick: 0,
            next_id: 0,
            ids: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: Entry) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tick += 1;
        let mut entry = entry;
        entry.usage = UsageRow {
            cum_mass: 0.0,
            admission_tick: self.tick,
        };
        self.entries.push(entry);
        self.ids.push(Some(id));
        self.enforce_budget();
        id
    }

    pub fn touch(&mut self, id: u64) -> bool {
        self.tick += 1;
        let tick = self.tick;
        match self.ids.iter().position(|&v| v == Some(id)) {
            Some(slot) => {
                observe(&mut self.entries[slot].usage, TOUCH_MASS, tick);
                true
            }
            None => false,
        }
    }

    pub fn render(&mut self) -> String {
        self.tick += 1;
        let tick = self.tick;
        let mut out = String::new();
        for e in self.entries.iter_mut() {
            observe(&mut e.usage, TOUCH_MASS, tick);
            out.push_str(e.kind.header());
            out.push('\n');
            out.push_str(&e.text);
            out.push('\n');
        }
        out
    }

    pub fn used_tokens(&self) -> usize {
        self.entries.iter().map(|e| e.tokens).sum()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries_ref(&self) -> &[Entry] {
        &self.entries
    }

    fn enforce_budget(&mut self) {
        while self.used_tokens() > self.max_tokens && self.entries.len() > MIN_ENTRIES_KEPT {
            let tick = self.tick;
            let pinned: Vec<bool> = self.entries.iter().map(Entry::is_pinned).collect();
            let scores: Vec<f32> = self.entries.iter().map(|e| e.uses(tick)).collect();
            let victims = select_evict(&scores, EVICT_BATCH, &pinned);
            let Some(&slot) = victims.first() else { break };
            self.entries.remove(slot);
            self.ids.remove(slot);
        }
    }
}
