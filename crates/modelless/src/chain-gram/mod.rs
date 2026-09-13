//! Chain-gram: count-based n-gram chain over whitespace-separated words.
//! Context of `order` previous words predicts the next word. Zero-copy: all
//! tokens borrow from the training text.

use std::collections::HashMap;

pub const DEFAULT_ORDER: usize = 2;

pub struct ChainGram<'a> {
    order: usize,
    counts: HashMap<Vec<&'a str>, Vec<(&'a str, usize)>>,
}

fn add_windows<'a>(
    words: &[&'a str],
    order: usize,
    counts: &mut HashMap<Vec<&'a str>, Vec<(&'a str, usize)>>,
) {
    if words.len() <= order {
        return;
    }
    let mut raw: HashMap<Vec<&'a str>, HashMap<&'a str, usize>> = HashMap::new();
    for win in words.windows(order + 1) {
        let (next, ctx) = win.split_last().expect("windows of order+1 is never empty");
        *raw.entry(ctx.to_vec())
            .or_default()
            .entry(next)
            .or_default() += 1;
    }
    for (ctx, nexts) in raw {
        let mut rows: Vec<(&'a str, usize)> = nexts.into_iter().collect();
        rows.sort();
        let row = counts.entry(ctx).or_default();
        for (token, count) in rows {
            match row.binary_search_by_key(&token, |(t, _)| *t) {
                Ok(i) => row[i].1 += count,
                Err(i) => row.insert(i, (token, count)),
            }
        }
    }
}

impl<'a> ChainGram<'a> {
    /// Train over text: each line is an independent token sequence, so
    /// chains never cross line boundaries.
    pub fn train(text: &'a str, order: usize) -> Self {
        let mut counts: HashMap<Vec<&'a str>, Vec<(&'a str, usize)>> = HashMap::new();
        for line in text.lines() {
            let words: Vec<&'a str> = line.split_whitespace().collect();
            add_windows(&words, order, &mut counts);
        }
        Self { order, counts }
    }

    /// Train over one continuous token sequence (e.g. corpus word order).
    pub fn train_seq(words: &[&'a str], order: usize) -> Self {
        let mut counts: HashMap<Vec<&'a str>, Vec<(&'a str, usize)>> = HashMap::new();
        add_windows(words, order, &mut counts);
        Self { order, counts }
    }

    pub fn order(&self) -> usize {
        self.order
    }

    pub fn context_count(&self) -> usize {
        self.counts.len()
    }

    /// Continuation counts for a context, sorted by token.
    pub fn candidates<'b>(&'b self, context: &[&'b str]) -> &'b [(&'a str, usize)] {
        self.counts
            .get(context)
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
    }

    /// Most frequent continuation of the context.
    pub fn next(&self, context: &[&str]) -> Option<&'a str> {
        let rows = self.candidates(context);
        rows.iter().max_by_key(|(_, c)| *c).map(|(token, _)| *token)
    }

    /// Chain forward from `seed` (last `order` words used) for `len` steps,
    /// stopping when a context has no continuation.
    pub fn generate(&self, seed: &[&'a str], len: usize) -> Vec<&'a str> {
        let mut ctx: Vec<&'a str> = seed.iter().rev().take(self.order).rev().copied().collect();
        let mut out = ctx.clone();
        for _ in 0..len {
            let Some(next) = self.next(&ctx) else { break };
            out.push(next);
            if self.order > 0 {
                ctx.remove(0);
                ctx.push(next);
            }
        }
        out
    }
}
