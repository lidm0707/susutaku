//! Single-chunk quantized KV cache (plan 06). One packed tensor per layer:
//! on append the old chunk is dequantized, concatenated with the new raw
//! positions and the whole is re-quantized (the mlx-lm QuantizedKVCache
//! approach; mild error compounding, O(1) graph nodes per step instead of
//! O(steps)). Packing is along head_dim (last axis), leaving the position
//! axis (2) indexable for truncation and sliding-window drops.

use mlx_rs::{
    Array,
    ops::indexing::IndexOp,
    ops::{concatenate_axis, dequantize, quantize},
    transforms::eval,
};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

/// Bits per KV element (user requirement: under 4-bit).
pub(crate) const KV_BITS: i32 = 3;
/// Elements sharing one scale/bias. head_dim (128/256/512) is divisible.
pub(crate) const KV_GROUP: i32 = 32;

/// One packed tensor's worth of quantized positions (keys or values).
pub struct QuantKv {
    /// packed weights, `[b, h, l, d / (32 / bits)]` u32
    w: Option<Array>,
    scales: Option<Array>,
    biases: Option<Array>,
    count: usize,
}

impl QuantKv {
    pub(crate) fn new() -> Self {
        Self {
            w: None,
            scales: None,
            biases: None,
            count: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Append one forward call's worth of positions: dequantize + concat +
    /// requantize the single chunk.
    pub(crate) fn append(&mut self, raw: &Array) -> Ex<()> {
        let (w, s, b) = match (&self.w, &self.scales, &self.biases) {
            (Some(w), Some(s), Some(b)) => {
                let old = dequantize(w, s, b, KV_GROUP, KV_BITS)?;
                let all = concatenate_axis(&[&old, raw], 2)?;
                quantize(&all, KV_GROUP, KV_BITS)?
            }
            _ => quantize(raw, KV_GROUP, KV_BITS)?,
        };
        self.count += raw.shape()[2] as usize;
        self.w = Some(w);
        self.scales = Some(s);
        self.biases = Some(b);
        Ok(())
    }

    /// Dequantize the single chunk (or `None` when empty).
    pub(crate) fn read(&self) -> Ex<Option<Array>> {
        match (&self.w, &self.scales, &self.biases) {
            (Some(w), Some(s), Some(b)) => Ok(Some(dequantize(w, s, b, KV_GROUP, KV_BITS)?)),
            _ => Ok(None),
        }
    }

    /// Materialize the packed arrays (called once per decode step so the
    /// lazy graph never stacks up).
    pub(crate) fn eval(&self) -> Ex<()> {
        if let (Some(w), Some(s), Some(b)) = (&self.w, &self.scales, &self.biases) {
            eval([w, s, b])?;
        }
        Ok(())
    }

    /// Keep only the first `kept` positions (spec-decode rollback).
    pub(crate) fn truncate(&mut self, kept: usize) {
        if self.count <= kept {
            return;
        }
        let end = kept as i32;
        let trim = |a: &Array| a.index((.., .., ..end));
        if let Some(w) = &self.w {
            self.w = Some(trim(w));
        }
        if let Some(s) = &self.scales {
            self.scales = Some(trim(s));
        }
        if let Some(b) = &self.biases {
            self.biases = Some(trim(b));
        }
        self.count = kept;
    }

    /// Drop the oldest `drop` positions (sliding window).
    pub(crate) fn drop_front(&mut self, drop: usize) {
        let drop = drop.min(self.count);
        if drop == 0 {
            return;
        }
        let start = drop as i32;
        let shift = |a: &Array| a.index((.., .., start..));
        if let Some(w) = &self.w {
            self.w = Some(shift(w));
        }
        if let Some(s) = &self.scales {
            self.scales = Some(shift(s));
        }
        if let Some(b) = &self.biases {
            self.biases = Some(shift(b));
        }
        self.count -= drop;
    }

    /// Keep the first `sink` and the last `recent` positions, dropping the
    /// middle. Packing is along head_dim, so packed position-axis slices
    /// concatenate exactly — no dequant/requant, and keys keep their
    /// absolute RoPE. No-op while under budget.
    pub(crate) fn compact(&mut self, sink: usize, recent: usize) -> Ex<()> {
        if self.count <= sink + recent {
            return Ok(());
        }
        let s = sink as i32;
        let r = (self.count - recent) as i32;
        let pick =
            |a: &Array| concatenate_axis(&[&a.index((.., .., ..s)), &a.index((.., .., r..))], 2);
        if let Some(w) = &self.w {
            self.w = Some(pick(w)?);
        }
        if let Some(s) = &self.scales {
            self.scales = Some(pick(s)?);
        }
        if let Some(b) = &self.biases {
            self.biases = Some(pick(b)?);
        }
        self.count = sink + recent;
        Ok(())
    }
}

/// Quantized key + value pair for one layer.
pub struct QuantKvPair {
    pub k: QuantKv,
    pub v: QuantKv,
    /// Absolute cached position (rope offset). Survives compact — stored
    /// keys keep their original positions — and rolls back on truncate.
    pub pos: i32,
}

impl Default for QuantKvPair {
    fn default() -> Self {
        Self::new()
    }
}

impl QuantKvPair {
    pub fn new() -> Self {
        Self {
            k: QuantKv::new(),
            v: QuantKv::new(),
            pos: 0,
        }
    }
    pub fn append(&mut self, k: &Array, v: &Array) -> Ex<()> {
        self.k.append(k)?;
        self.v.append(v)?;
        self.pos += k.shape()[2];
        Ok(())
    }

    /// `(keys, values)` over all cached positions, or `None` when empty.
    pub fn read(&self) -> Ex<Option<(Array, Array)>> {
        match (self.k.read()?, self.v.read()?) {
            (Some(k), Some(v)) => Ok(Some((k, v))),
            _ => Ok(None),
        }
    }

    pub(crate) fn eval(&self) -> Ex<()> {
        self.k.eval()?;
        self.v.eval()?;
        Ok(())
    }

    pub fn truncate(&mut self, kept: usize) {
        let dropped = self.k.len().saturating_sub(kept) as i32;
        self.k.truncate(kept);
        self.v.truncate(kept);
        self.pos -= dropped;
    }

    /// Sink + recent compaction (see QuantKv::compact); `pos` unchanged.
    pub fn compact(&mut self, sink: usize, recent: usize) -> Ex<()> {
        self.k.compact(sink, recent)?;
        self.v.compact(sink, recent)
    }
}
