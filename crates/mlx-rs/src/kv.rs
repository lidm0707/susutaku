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
pub(crate) struct QuantKv {
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

    pub(crate) fn len(&self) -> usize {
        self.count
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
pub(crate) struct QuantKvPair {
    pub(crate) k: QuantKv,
    pub(crate) v: QuantKv,
    /// Absolute cached position (rope offset). Survives compact — stored
    /// keys keep their original positions — and rolls back on truncate.
    pub(crate) pos: i32,
}

impl QuantKvPair {
    pub(crate) fn new() -> Self {
        Self {
            k: QuantKv::new(),
            v: QuantKv::new(),
            pos: 0,
        }
    }
    pub(crate) fn append(&mut self, k: &Array, v: &Array) -> Ex<()> {
        self.k.append(k)?;
        self.v.append(v)?;
        self.pos += k.shape()[2];
        Ok(())
    }

    /// `(keys, values)` over all cached positions, or `None` when empty.
    pub(crate) fn read(&self) -> Ex<Option<(Array, Array)>> {
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

    pub(crate) fn truncate(&mut self, kept: usize) {
        let dropped = self.k.len().saturating_sub(kept) as i32;
        self.k.truncate(kept);
        self.v.truncate(kept);
        self.pos -= dropped;
    }

    /// Sink + recent compaction (see QuantKv::compact); `pos` unchanged.
    pub(crate) fn compact(&mut self, sink: usize, recent: usize) -> Ex<()> {
        self.k.compact(sink, recent)?;
        self.v.compact(sink, recent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(l: i32) -> Array {
        // head_dim must be divisible by KV_GROUP (32); distinct per position
        // so slice-order mistakes are visible
        let data: Vec<f32> = (0..l * 32).map(|i| i as f32).collect();
        Array::from_slice(&data, &[1, 1, l, 32])
    }

    #[test]
    fn compact_keeps_sink_and_recent() {
        let mut pair = QuantKvPair::new();
        pair.append(&raw(10), &raw(10)).unwrap();
        assert_eq!(pair.pos, 10);
        pair.compact(2, 3).unwrap();
        assert_eq!(pair.k.len(), 5);
        assert_eq!(pair.v.len(), 5);
        assert_eq!(pair.pos, 10, "absolute positions must survive compaction");
        // stored keys must be exactly the first 2 and last 3 positions
        let got = pair.read().unwrap().unwrap();
        let s = got.0.as_slice::<f32>();
        // 3-bit quantization is lossy — step ≈ 31/7, so assert within half a step
        let near = |a: f32, b: f32| (a - b).abs() < 2.3;
        for (i, g) in s[..32].iter().enumerate() {
            assert!(near(*g, i as f32), "sink pos 0 elem {i}: {g}");
        }
        for (i, g) in s[2 * 32..].iter().enumerate() {
            assert!(near(*g, (7 * 32 + i) as f32), "recent elem {i}: {g}");
        }
    }

    #[test]
    fn compact_under_budget_is_noop() {
        let mut pair = QuantKvPair::new();
        pair.append(&raw(4), &raw(4)).unwrap();
        pair.compact(2, 3).unwrap();
        assert_eq!(pair.k.len(), 4);
        assert_eq!(pair.pos, 4);
    }

    #[test]
    fn truncate_rolls_back_absolute_pos() {
        let mut pair = QuantKvPair::new();
        pair.append(&raw(6), &raw(6)).unwrap();
        pair.truncate(4);
        assert_eq!(pair.k.len(), 4);
        assert_eq!(pair.pos, 4);
    }
}
