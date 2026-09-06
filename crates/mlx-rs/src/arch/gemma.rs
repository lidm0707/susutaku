//! gemma4 text stack: alternating sliding/full attention, shared MLP + 128-expert
//! top-8 MoE per layer, per-layer scalars, tied embeddings, logit softcap.
//! Reference: mlx_lm/models/gemma4_text.py.

use std::collections::HashMap;
use std::path::Path;

use mlx_rs::{
    Array, Dtype, array,
    fast::{ScaledDotProductAttentionMask, rms_norm, scaled_dot_product_attention},
    module::{Module, Param},
    nn,
    ops::indexing::{IndexOp, take_along_axis},
    ops::{argpartition_axis, concatenate_axis, quantized_matmul, softmax_axis, tanh, zeros},
};

use crate::quant::{
    MaybeEmbed, MaybeLinear, embed_from_bits, linear_from_bits, load_map, req, rms, rms_scale,
};

type Ex<T> = Result<T, Box<dyn std::error::Error>>;

/// Gemma4 "proportional" rope (mlx_lm ProportionalRoPE): pair i (i <
/// rotated/2) rotates with angle pos * base^(-2i/dims) — frequencies span the
/// FULL head dim — and the remaining dims pass through.
struct PropRope {
    dims: i32,
    rotated: i32,
    base: f32,
}

impl PropRope {
    fn new(dims: i32, rotated: i32, base: f32) -> Self {
        Self {
            dims,
            rotated,
            base,
        }
    }

    fn forward(&self, x: &Array, offset: i32) -> Ex<Array> {
        // freqs[i] = base^(2i/dims) for rotated pairs; inf => angle 0 for the
        // passthrough pairs. mx.fast.rope (traditional=false) rotates
        // interleaved pairs (2i, 2i+1), matching mlx-lm exactly.
        let pairs = (self.dims / 2) as usize;
        let rot_pairs = (self.rotated / 2) as usize;
        let mut f = Vec::with_capacity(pairs);
        for i in 0..rot_pairs {
            let e = 2.0 * i as f32 / self.dims as f32;
            f.push(self.base.powf(e));
        }
        f.resize(pairs, f32::INFINITY);
        let freqs = Array::from_slice(&f, &[pairs as i32]);
        Ok(mlx_rs::fast::rope(
            x,
            self.dims,
            false,
            None,
            1.0,
            offset,
            Some(&freqs),
        )?)
    }
}

// ---------- config ----------
struct GConfig {
    layers: usize,
    hidden: i32,
    heads: i32,
    head_dim: i32,
    global_head_dim: i32,
    kv_heads: i32,
    global_kv_heads: i32,
    eps: f32,
    num_experts: i32,
    top_k: usize,
    window: usize,
    theta_sliding: f32,
    theta_full: f32,
    rot_sliding: i32,
    rot_full: i32,
    group_size: i32,
    bits: i32,
    softcap: Option<f32>,
    is_full: Vec<bool>,
    k_eq_v: bool,
    /// E4B per-layer-embedding width (0 = not a per-layer-input model).
    pli_hidden: i32,
    /// Layers past this index reuse K/V from an earlier layer of the same
    /// type (`layers - num_kv_shared_layers`; usize::MAX = never).
    first_kv_shared: usize,
    /// For each layer, the index whose K/V it reuses (== itself when owner).
    previous_kvs: Vec<usize>,
}

impl GConfig {
    fn parse(json: &serde_json::Value) -> Ex<Self> {
        let t = if json.get("text_config").is_some_and(|t| !t.is_null()) {
            &json["text_config"]
        } else {
            json
        };
        let v = |k: &str| t[k].as_i64().unwrap_or_default() as i32;
        let rope = |kind: &str| &t["rope_parameters"][kind];
        let f = |j: &serde_json::Value, k: &str| j[k].as_f64().unwrap_or_default() as f32;

        let head_dim = v("head_dim");
        let global_head_dim = if v("global_head_dim") > 0 {
            v("global_head_dim")
        } else {
            head_dim
        };
        let partial_sliding = rope("sliding_attention")["partial_rotary_factor"]
            .as_f64()
            .unwrap_or(1.0) as f32;
        let partial_full = rope("full_attention")["partial_rotary_factor"]
            .as_f64()
            .unwrap_or(1.0) as f32;

        let is_full: Vec<bool> = t["layer_types"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| x.as_str() == Some("full_attention"))
                    .collect()
            })
            .unwrap_or_else(|| {
                let interval = v("sliding_window_pattern").max(1) as usize;
                (0..v("num_hidden_layers") as usize)
                    .map(|i| i % interval == interval - 1)
                    .collect()
            });

        // KV sharing (E4B): the last `num_kv_shared_layers` layers reuse the
        // K/V of the last earlier layer of the same attention type.
        let n = v("num_hidden_layers") as usize;
        let shared = v("num_kv_shared_layers") as usize;
        let first_kv_shared = if shared > 0 && shared < n {
            n - shared
        } else {
            usize::MAX
        };
        let mut previous_kvs: Vec<usize> = (0..n).collect();
        if first_kv_shared != usize::MAX {
            let mut owners: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for (i, kind) in is_full.iter().enumerate().take(first_kv_shared) {
                owners.insert(if *kind { "full" } else { "sliding" }, i);
            }
            for j in first_kv_shared..n {
                let kind = if is_full[j] { "full" } else { "sliding" };
                previous_kvs[j] = owners[kind];
            }
        }

        Ok(Self {
            layers: v("num_hidden_layers") as usize,
            hidden: v("hidden_size"),
            heads: v("num_attention_heads"),
            head_dim,
            global_head_dim,
            kv_heads: v("num_key_value_heads"),
            global_kv_heads: v("num_global_key_value_heads"),
            eps: t["rms_norm_eps"].as_f64().unwrap_or(1e-6) as f32,
            num_experts: v("num_experts"),
            top_k: v("top_k_experts") as usize,
            window: v("sliding_window") as usize,
            theta_sliding: f(rope("sliding_attention"), "rope_theta"),
            theta_full: f(rope("full_attention"), "rope_theta"),
            rot_sliding: (head_dim as f32 * partial_sliding) as i32,
            rot_full: (global_head_dim as f32 * partial_full) as i32,
            group_size: json["quantization"]["group_size"].as_i64().unwrap_or(64) as i32,
            bits: json["quantization"]["bits"].as_i64().unwrap_or(4) as i32,
            softcap: t["final_logit_softcapping"].as_f64().map(|c| c as f32),
            is_full,
            k_eq_v: t["attention_k_eq_v"].as_bool().unwrap_or(false),
            pli_hidden: v("hidden_size_per_layer_input"),
            first_kv_shared,
            previous_kvs,
        })
    }
}

// ---------- attention ----------

struct Attn {
    q_proj: MaybeLinear,
    /// None for KV-shared layers (they reuse an earlier full layer's K/V)
    k_proj: Option<MaybeLinear>,
    v_proj: Option<MaybeLinear>,
    o_proj: MaybeLinear,
    q_norm: nn::RmsNorm,
    k_norm: Option<nn::RmsNorm>,
    rope: PropRope,
    heads: i32,
    kv_heads: i32,
    head_dim: usize,
    sliding: bool,
    window: usize,
}

pub(crate) struct RollCache {
    kv: crate::kv::QuantKvPair,
    pub(crate) count: i32,
}

pub(crate) enum AttnCache {
    Full(crate::kv::QuantKvPair),
    Roll(RollCache),
}

impl AttnCache {
    /// Materialize the packed cache arrays (once per decode step).
    pub(crate) fn eval(&self) -> Ex<()> {
        match self {
            AttnCache::Full(c) => c.eval(),
            AttnCache::Roll(r) => r.kv.eval(),
        }
    }

    /// Drop cached positions beyond `kept` (speculative-decode rollback).
    pub(crate) fn truncate(&mut self, kept: usize) {
        match self {
            AttnCache::Full(c) => c.truncate(kept),
            AttnCache::Roll(r) => {
                let len = r.kv.k.len();
                if len > kept {
                    r.kv.truncate(kept);
                    // count is the absolute position counter; roll back by
                    // the dropped count
                    r.count -= (len - kept) as i32;
                }
            }
        }
    }
}

impl Attn {
    /// KV-shared layers (E4B layers past `num_kv_shared_layers`) receive the
    /// roped keys/values from the earlier full-attention layer that owns the
    /// cache; they compute only their own queries.
    fn forward(
        &mut self,
        x: &Array,
        shared_kv: Option<(&Array, &Array, i32)>,
        cache: &mut AttnCache,
    ) -> Ex<(Array, Option<KvIntermediate>)> {
        let b = x.shape()[0];
        let l = x.shape()[1];
        // head dim is per-layer-type: shared layers borrow K/V of their own
        // type (sliding 256 / full 512), so dims always line up
        let d = self.head_dim as i32;

        let q = self.q_proj.forward(x)?.reshape(&[b, l, self.heads, d])?;
        let q = self.q_norm.forward(&q)?;
        let mut q = q.transpose_axes(&[0, 2, 1, 3])?;

        let (keys, values, cached, shared_pos) = match shared_kv {
            Some((kc, vc, offset)) => {
                q = self.rope.forward(&q, offset)?;
                (kc.clone(), vc.clone(), (kc.shape()[2] - l) as usize, offset)
            }
            None => {
                let raw_k =
                    self.k_proj
                        .as_mut()
                        .unwrap()
                        .forward(x)?
                        .reshape(&[b, l, self.kv_heads, d])?;
                let mut v = match self.v_proj.as_mut() {
                    Some(vp) => vp.forward(x)?.reshape(&[b, l, self.kv_heads, d])?,
                    None => raw_k.clone(),
                };
                let k = self.k_norm.as_mut().unwrap().forward(&raw_k)?;
                let mut k = k.transpose_axes(&[0, 2, 1, 3])?;
                v = rms_scale(&v, 1.0)?.as_dtype(raw_k.dtype())?;
                v = v.transpose_axes(&[0, 2, 1, 3])?;

                let offset = match cache {
                    AttnCache::Full(c) => c.pos,
                    AttnCache::Roll(r) => r.count,
                };
                q = self.rope.forward(&q, offset)?;
                k = self.rope.forward(&k, offset)?;

                // cached history (dequantized) + the new block, for attention
                let (mut keys, mut values) = match cache {
                    AttnCache::Full(c) => match c.read()? {
                        Some((kc, vc)) => (
                            concatenate_axis(&[&kc, &k], 2)?,
                            concatenate_axis(&[&vc, &v], 2)?,
                        ),
                        None => (k.clone(), v.clone()),
                    },
                    AttnCache::Roll(r) => match r.kv.read()? {
                        Some((kc, vc)) => (
                            concatenate_axis(&[&kc, &k], 2)?,
                            concatenate_axis(&[&vc, &v], 2)?,
                        ),
                        None => (k.clone(), v.clone()),
                    },
                };
                match cache {
                    AttnCache::Full(c) => c.append(&k, &v)?,
                    AttnCache::Roll(r) => {
                        r.count += l;
                        r.kv.append(&k, &v)?;
                        // sliding window: drop the oldest positions past the
                        // window; the sliding mask hides anything the window
                        // cut anyway
                        let over = r.kv.k.len().saturating_sub(self.window);
                        r.kv.k.drop_front(over);
                        r.kv.v.drop_front(over);
                        // attention sees the same trimmed window the cache
                        // keeps (mlx-lm RotatingKVCache semantics)
                        let kl = keys.shape()[2] as usize;
                        let over = kl.saturating_sub(self.window);
                        if over > 0 {
                            let start = over as i32;
                            keys = keys.index((.., .., start..));
                            values = values.index((.., .., start..));
                        }
                    }
                };
                (
                    keys.clone(),
                    values.clone(),
                    (keys.shape()[2] as usize).saturating_sub(l as usize),
                    offset,
                )
            }
        };
        let mask: Option<Array> = if l > 1 || cached > 0 {
            Some(if self.sliding {
                sliding_mask_offset(l as usize, cached, self.window, q.dtype())?
            } else {
                causal_mask_offset(l as usize, cached, q.dtype())?
            })
        } else {
            None
        };
        let out = match mask.as_ref() {
            Some(m) => scaled_dot_product_attention(
                &q,
                &keys,
                &values,
                1.0,
                ScaledDotProductAttentionMask::Array(m),
            )?,
            None => scaled_dot_product_attention(&q, &keys, &values, 1.0, None)?,
        };
        let out = out.transpose_axes(&[0, 2, 1, 3])?.reshape(&[b, l, -1])?;
        let out = self.o_proj.forward(&out)?;
        let kv = match shared_kv {
            // pre-update offset: shared consumers rope queries at the first
            // new position, matching the owner's roped keys
            None => Some(KvIntermediate {
                keys,
                values,
                pos: shared_pos,
            }),
            Some(_) => None,
        };
        Ok((out, kv))
    }
}

/// Causal mask over `cached` history positions + `l` new positions.
/// Shape `(l, cached + l)`: all history visible, causal within the new block.
fn causal_mask_offset(l: usize, cached: usize, dtype: Dtype) -> Ex<Array> {
    let total = cached + l;
    let mut data = Vec::with_capacity(l * total);
    for i in 0..l {
        for j in 0..total {
            let visible = j < cached || j - cached <= i;
            data.push(if visible { 0.0 } else { f32::NEG_INFINITY });
        }
    }
    Ok(Array::from_slice(&data, &[l as i32, total as i32]).as_dtype(dtype)?)
}

/// Sliding-window mask with `cached` history positions prepended. Query `i`
/// sits at absolute position `cached + i`; a key at absolute position `j` is
/// visible when `cached + i - j < window`.
fn sliding_mask_offset(l: usize, cached: usize, window: usize, dtype: Dtype) -> Ex<Array> {
    let total = cached + l;
    let mut data = Vec::with_capacity(l * total);
    for i in 0..l {
        let pos = cached + i;
        for j in 0..total {
            let visible = j <= pos && pos - j < window;
            data.push(if visible { 0.0 } else { f32::NEG_INFINITY });
        }
    }
    Ok(Array::from_slice(&data, &[l as i32, total as i32]).as_dtype(dtype)?)
}

// ---------- moe ----------

struct GemmaMlp {
    gate_proj: MaybeLinear,
    up_proj: MaybeLinear,
    down_proj: MaybeLinear,
}

impl GemmaMlp {
    fn forward(&mut self, x: &Array) -> Ex<Array> {
        let gate = nn::gelu_approximate(self.gate_proj.forward(x)?)?;
        let up = self.up_proj.forward(x)?;
        Ok(self.down_proj.forward(&gate.multiply(&up)?)?)
    }
}

impl Gemma {
    fn embed_dtype(&self) -> Dtype {
        match &self.embed {
            MaybeEmbed::Quantized(q) => q.inner.weight.dtype(),
            MaybeEmbed::Original(e) => e.weight.dtype(),
        }
    }
}

struct Moe {
    proj: MaybeLinear,
    scale: Param<Array>,
    per_expert: Param<Array>,
    gate_w: Param<Array>,
    gate_s: Param<Array>,
    gate_b: Param<Array>,
    up_w: Param<Array>,
    up_s: Param<Array>,
    up_b: Param<Array>,
    down_w: Param<Array>,
    down_s: Param<Array>,
    down_b: Param<Array>,
    group: i32,
    bits: i32,
    top_k: usize,
}

impl Moe {
    fn expert_forward(&self, x: &Array, e: i32) -> Ex<Array> {
        let (gw, gs, gb) = (
            self.gate_w.index((e, .., ..)),
            self.gate_s.index((e, .., ..)),
            self.gate_b.index((e, .., ..)),
        );
        let (uw, us, ub) = (
            self.up_w.index((e, .., ..)),
            self.up_s.index((e, .., ..)),
            self.up_b.index((e, .., ..)),
        );
        let (dw, ds, db) = (
            self.down_w.index((e, .., ..)),
            self.down_s.index((e, .., ..)),
            self.down_b.index((e, .., ..)),
        );
        let gate = quantized_matmul(x, &gw, &gs, &gb, true, self.group, self.bits)?;
        let up = quantized_matmul(x, &uw, &us, &ub, true, self.group, self.bits)?;
        let h = nn::gelu_approximate(gate)?.multiply(&up)?;
        quantized_matmul(&h, &dw, &ds, &db, true, self.group, self.bits).map_err(Into::into)
    }

    fn router(&mut self, h: &Array, hidden: i32, eps: f32) -> Ex<(Array, Array)> {
        let root = (hidden as f32).powf(-0.5);
        let sw = self
            .scale
            .multiply(&array!(root).as_dtype(self.scale.dtype())?)?;
        let normed = rms_norm(h, &sw, eps)?;
        let scores = self.proj.forward(&normed)?;
        let e = self.per_expert.shape()[0];
        let idx_shape0 = scores.shape()[0];
        let part = argpartition_axis(&scores, e - self.top_k as i32, -1)?;
        // slice of argpartition output is non-contiguous; reshape forces a contiguous copy
        let idx = part
            .reshape(&[idx_shape0, scores.shape()[1], e])?
            .index((.., .., (e - self.top_k as i32)..))
            .as_dtype(Dtype::Uint32)?
            .reshape(&[idx_shape0, scores.shape()[1], self.top_k as i32])?;

        let mut weights = take_along_axis(&scores, &idx, -1)?;
        weights = softmax_axis(&weights, -1, None)?;
        let per = self.per_expert.index(idx.clone());
        let weights = weights.multiply(&per)?;
        Ok((idx, weights))
    }

    fn experts(&self, x: &Array, idx: &Array, weights: &Array) -> Ex<Array> {
        let b = x.shape()[0];
        let l = x.shape()[1];
        let hdim = x.shape()[2];
        let flat = x.reshape(&[b * l, hdim])?;
        let idx_flat = idx.reshape(&[b * l, self.top_k as i32])?;
        let w_flat = weights.reshape(&[b * l, self.top_k as i32])?;

        let idx_host: Vec<u32> = (0..(b * l) as usize)
            .flat_map(|rr| {
                let rowv = idx_flat
                    .index((rr as i32, ..))
                    .as_dtype(Dtype::Uint32)
                    .unwrap();
                rowv.as_slice::<u32>().to_vec()
            })
            .collect();
        let mut out = zeros::<f32>(&[b * l, hdim])?.as_dtype(x.dtype())?;
        for k in 0..self.top_k {
            let mut groups: HashMap<u32, Vec<usize>> = HashMap::new();
            for t in 0..(b * l) as usize {
                groups
                    .entry(idx_host[t * self.top_k + k])
                    .or_default()
                    .push(t);
            }
            for (e, rows) in groups {
                let n = rows.len() as i32;
                let pos: Vec<u32> = rows.iter().map(|r| *r as u32).collect();
                let xs = flat.index(Array::from_slice(&pos, &[n]));
                let y = self.expert_forward(&xs, e as i32)?;
                let wk = w_flat
                    .index((.., k as i32))
                    .index(Array::from_slice(&pos, &[n]))
                    .reshape(&[n, 1])?;
                let yw = y.multiply(&wk)?;
                let slot = zeros::<f32>(&[b * l, hdim])?.as_dtype(x.dtype())?;
                let slot = slot
                    .put_along_axis(Array::from_slice(&pos, &[n, 1]), &yw, Some(0))
                    .map_err(Box::new)?;
                out = out.add(&slot)?;
            }
        }
        Ok(out.reshape(&[b, l, hdim])?)
    }
}

// ---------- layer & model ----------

/// E4B per-layer input: a global embedding table sliced per layer, gated
/// against that layer's post-attention hidden state and projected back into
/// the residual stream (gated multiplicatively with the token's own slice).
struct PerLayerInput {
    gate: MaybeLinear,
    proj: MaybeLinear,
    norm: nn::RmsNorm,
}

impl PerLayerInput {
    fn forward(&mut self, slice: &Array, gate_in: &Array) -> Ex<Array> {
        // reference order: gelu(gate(h)) * slice → project → norm(hidden)
        let g = nn::gelu_approximate(self.gate.forward(gate_in)?)?;
        let g = slice.multiply(&g)?;
        let p = self.proj.forward(&g)?;
        Ok(self.norm.forward(&p)?)
    }
}

struct Layer {
    attn: Attn,
    mlp: GemmaMlp,
    ln_input: nn::RmsNorm,
    ln_post_attn: nn::RmsNorm,
    ln_pre_ff: nn::RmsNorm,
    ln_post_ff: nn::RmsNorm,
    moe: Option<Moe>,
    ln_post_ff_1: Option<nn::RmsNorm>,
    ln_post_ff_2: Option<nn::RmsNorm>,
    ln_pre_ff_2: Option<nn::RmsNorm>,
    layer_scalar: Param<Array>,
    pli: Option<PerLayerInput>,
}

/// K/V intermediates a KV-shared layer consumes: roped keys (incl. the new
/// block), values, and the owner's absolute rope offset after append.
struct KvIntermediate {
    keys: Array,
    values: Array,
    pos: i32,
}

impl Layer {
    #[allow(clippy::type_complexity)]
    fn forward(
        &mut self,
        x: &Array,
        pli_slice: Option<&Array>,
        shared_kv: Option<(&Array, &Array, i32)>,
        cache: &mut AttnCache,
    ) -> Ex<(Array, Option<KvIntermediate>)> {
        let residual = x;
        let h = self.ln_input.forward(x)?;
        let (attn_out, kv) = self.attn.forward(&h, shared_kv, cache)?;
        let gate_in = self.ln_post_attn.forward(&attn_out)?;
        let h = residual.add(&gate_in)?;

        let residual = h.clone();
        let h = match self.moe.as_mut() {
            Some(moe) => {
                let hidden = x.shape()[2];
                let eps = self.ln_pre_ff.eps;
                let pre = self.ln_pre_ff.forward(&h.clone())?;
                let ff = self.mlp.forward(&pre)?;
                let h1 = self.ln_post_ff_1.as_mut().unwrap().forward(&ff)?;
                let h2 = self.ln_pre_ff_2.as_mut().unwrap().forward(&h)?;
                let (idx, w) = moe.router(&h, hidden, eps)?;
                let h2 = moe.experts(&h2, &idx, &w)?;
                let h2 = self.ln_post_ff_2.as_mut().unwrap().forward(&h2)?;
                h1.add(&h2)?
            }
            None => self.mlp.forward(&self.ln_pre_ff.forward(&h)?)?,
        };
        let h = self.ln_post_ff.forward(&h)?;
        let mut h = residual.add(&h)?;
        // per-layer gate reads the post-FF residual stream (mlx-lm DecoderLayer)
        if let (Some(pli), Some(slice)) = (&mut self.pli, pli_slice) {
            let p = pli.forward(slice, &h)?;
            h = h.add(&p)?;
        }
        let out = h.multiply(&self.layer_scalar)?;
        Ok((out, kv))
    }
}

pub struct Gemma {
    embed: MaybeEmbed,
    embed_scale: f32,
    layers: Vec<Layer>,
    norm: nn::RmsNorm,
    pub(crate) caches: Vec<AttnCache>,
    is_full: Vec<bool>,
    softcap: Option<f32>,
    /// E4B: token ids → pli table, scaled, split into per-layer slices.
    pli_embed: Option<MaybeEmbed>,
    pli_embed_scale: f32,
    /// E4B: hidden states → per-layer slices (added to the embed slices).
    pli_model_proj: Option<MaybeLinear>,
    pli_model_scale: f32,
    pli_input_scale: f32,
    /// RMSNorm weight applied to the projected per-layer slices
    /// (E4B only; absent on the 26B MoE checkpoint)
    pli_proj_norm: Option<Param<Array>>,
    pli_norm_eps: f32,
    pli_hidden: i32,
    previous_kvs: Vec<usize>,
}

impl Gemma {
    pub fn load(dir: &Path, json: &serde_json::Value) -> Ex<Self> {
        let cfg = GConfig::parse(json)?;
        let map = load_map(dir)?;
        // E4B: per-layer input components, shared struct per layer (embed is
        // the same table every layer slices)
        let pli_enabled = cfg.pli_hidden > 0 && map.contains_key("embed_tokens_per_layer.weight");
        let pli = if pli_enabled { Some(()) } else { None };
        let layers = (0..cfg.layers)
            .map(|i| {
                let p = format!("layers.{i}.");
                let ap = format!("{p}self_attn.");
                let full = cfg.is_full[i];
                let head_dim = if full {
                    cfg.global_head_dim
                } else {
                    cfg.head_dim
                };
                let kv_heads = if full && cfg.global_kv_heads > 0 {
                    cfg.global_kv_heads
                } else {
                    cfg.kv_heads
                };
                let k_eq_v = cfg.k_eq_v && full;
                let has_kv = i < cfg.first_kv_shared;
                let (theta, rot) = if full {
                    (cfg.theta_full, cfg.rot_full)
                } else {
                    (cfg.theta_sliding, cfg.rot_sliding)
                };
                let (group, bits) = (cfg.group_size, cfg.bits);
                let qbits =
                    json["quantization"][&format!("language_model.model.{p}router.proj")]["bits"]
                        .as_i64()
                        .unwrap_or(bits as i64) as i32;

                let moe = if cfg.num_experts > 0
                    && map.contains_key(&format!("{p}router.proj.weight"))
                {
                    let rp = format!("{p}router.");
                    let ep = format!("{p}experts.switch_glu.");
                    let gbits = |k: &str| {
                        json["quantization"][&format!("language_model.model.{ep}{k}")]["bits"]
                            .as_i64()
                            .unwrap_or(bits as i64) as i32
                    };
                    let ggroup = |k: &str| {
                        json["quantization"][&format!("language_model.model.{ep}{k}")]["group_size"]
                            .as_i64()
                            .unwrap_or(group as i64) as i32
                    };
                    Some(Moe {
                        proj: linear_from_bits(&map, &format!("{rp}proj"), group, qbits)?,
                        scale: Param::new(req(&map, &format!("{rp}scale"))?),
                        per_expert: Param::new(req(&map, &format!("{rp}per_expert_scale"))?),
                        gate_w: Param::new(req(&map, &format!("{ep}gate_proj.weight"))?),
                        gate_s: Param::new(req(&map, &format!("{ep}gate_proj.scales"))?),
                        gate_b: Param::new(req(&map, &format!("{ep}gate_proj.biases"))?),
                        up_w: Param::new(req(&map, &format!("{ep}up_proj.weight"))?),
                        up_s: Param::new(req(&map, &format!("{ep}up_proj.scales"))?),
                        up_b: Param::new(req(&map, &format!("{ep}up_proj.biases"))?),
                        down_w: Param::new(req(&map, &format!("{ep}down_proj.weight"))?),
                        down_s: Param::new(req(&map, &format!("{ep}down_proj.scales"))?),
                        down_b: Param::new(req(&map, &format!("{ep}down_proj.biases"))?),
                        group: ggroup("gate_proj"),
                        bits: gbits("gate_proj"),
                        top_k: cfg.top_k,
                    })
                } else {
                    None
                };

                Ok(Layer {
                    attn: Attn {
                        q_proj: linear_from_bits(&map, &format!("{ap}q_proj"), group, bits)?,
                        k_proj: if has_kv {
                            Some(linear_from_bits(&map, &format!("{ap}k_proj"), group, bits)?)
                        } else {
                            None
                        },
                        v_proj: if has_kv && !k_eq_v {
                            Some(linear_from_bits(&map, &format!("{ap}v_proj"), group, bits)?)
                        } else {
                            None
                        },
                        o_proj: linear_from_bits(&map, &format!("{ap}o_proj"), group, bits)?,
                        q_norm: rms(req(&map, &format!("{ap}q_norm.weight"))?, cfg.eps),
                        k_norm: if has_kv {
                            Some(rms(req(&map, &format!("{ap}k_norm.weight"))?, cfg.eps))
                        } else {
                            None
                        },
                        rope: if full {
                            PropRope::new(head_dim, rot, theta)
                        } else {
                            // plain rope: freqs = base^(2i/rot)
                            PropRope::new(rot, rot, theta)
                        },
                        heads: cfg.heads,
                        kv_heads,
                        head_dim: head_dim as usize,
                        sliding: !full,
                        window: cfg.window,
                    },
                    mlp: GemmaMlp {
                        gate_proj: linear_from_bits(
                            &map,
                            &format!("{p}mlp.gate_proj"),
                            group,
                            bits,
                        )?,
                        up_proj: linear_from_bits(&map, &format!("{p}mlp.up_proj"), group, bits)?,
                        down_proj: linear_from_bits(
                            &map,
                            &format!("{p}mlp.down_proj"),
                            group,
                            bits,
                        )?,
                    },
                    ln_input: rms(req(&map, &format!("{p}input_layernorm.weight"))?, cfg.eps),
                    ln_post_attn: rms(
                        req(&map, &format!("{p}post_attention_layernorm.weight"))?,
                        cfg.eps,
                    ),
                    ln_pre_ff: rms(
                        req(&map, &format!("{p}pre_feedforward_layernorm.weight"))?,
                        cfg.eps,
                    ),
                    ln_post_ff: rms(
                        req(&map, &format!("{p}post_feedforward_layernorm.weight"))?,
                        cfg.eps,
                    ),
                    ln_post_ff_1: if moe.is_some() {
                        Some(rms(
                            req(&map, &format!("{p}post_feedforward_layernorm_1.weight"))?,
                            cfg.eps,
                        ))
                    } else {
                        None
                    },
                    ln_post_ff_2: if moe.is_some() {
                        Some(rms(
                            req(&map, &format!("{p}post_feedforward_layernorm_2.weight"))?,
                            cfg.eps,
                        ))
                    } else {
                        None
                    },
                    ln_pre_ff_2: if moe.is_some() {
                        Some(rms(
                            req(&map, &format!("{p}pre_feedforward_layernorm_2.weight"))?,
                            cfg.eps,
                        ))
                    } else {
                        None
                    },
                    moe,
                    layer_scalar: Param::new(req(&map, &format!("{p}layer_scalar"))?),
                    pli: match &pli {
                        Some(()) => Some(PerLayerInput {
                            gate: linear_from_bits(
                                &map,
                                &format!("{p}per_layer_input_gate"),
                                group,
                                bits,
                            )?,
                            norm: rms(
                                req(&map, &format!("{p}post_per_layer_input_norm.weight"))?,
                                cfg.eps,
                            ),
                            proj: linear_from_bits(
                                &map,
                                &format!("{p}per_layer_projection"),
                                group,
                                bits,
                            )?,
                        }),
                        None => None,
                    },
                })
            })
            .collect::<Ex<Vec<_>>>()?;

        Ok(Self {
            embed: embed_from_bits(&map, "embed_tokens", cfg.group_size, cfg.bits)?,
            embed_scale: (cfg.hidden as f32).sqrt(),
            norm: rms(req(&map, "norm.weight")?, cfg.eps),
            layers,
            caches: Vec::new(),
            is_full: cfg.is_full.clone(),
            softcap: cfg.softcap,
            pli_embed: if cfg.pli_hidden > 0 {
                Some(embed_from_bits(
                    &map,
                    "embed_tokens_per_layer",
                    cfg.group_size,
                    cfg.bits,
                )?)
            } else {
                None
            },
            pli_embed_scale: (cfg.pli_hidden as f32).sqrt(),
            pli_model_proj: if cfg.pli_hidden > 0 {
                Some(linear_from_bits(
                    &map,
                    "per_layer_model_projection",
                    cfg.group_size,
                    cfg.bits,
                )?)
            } else {
                None
            },
            pli_model_scale: (cfg.hidden as f32).powf(-0.5),
            pli_input_scale: 2.0_f32.powf(-0.5),
            pli_proj_norm: map
                .get("per_layer_projection_norm.weight")
                .map(|a| Param::new(a.clone())),
            pli_norm_eps: cfg.eps,
            pli_hidden: cfg.pli_hidden,
            previous_kvs: cfg.previous_kvs.clone(),
        })
    }

    pub fn forward(&mut self, tokens: &Array) -> Ex<Array> {
        let mut h = self
            .embed
            .forward(tokens)?
            .multiply(&array!(self.embed_scale).as_dtype(self.embed_dtype())?)?;

        // E4B: per-layer slices = (embed table lookup + hidden projection)
        // scaled, then split into one [b, l, pli] slice per layer.
        let pli_slices: Option<Vec<Array>> = if self.pli_hidden > 0 {
            let from_embed = self
                .pli_embed
                .as_mut()
                .unwrap()
                .forward(tokens)?
                .multiply(&array!(self.pli_embed_scale).as_dtype(mlx_rs::Dtype::Float32)?)?;
            let mut from_hidden = self.pli_model_proj.as_mut().unwrap().forward(&h)?;
            let out_dt = from_hidden.dtype();
            from_hidden = from_hidden.multiply(&array!(self.pli_model_scale).as_dtype(out_dt)?)?;
            let n = self.layers.len() as i32;
            let w = self.pli_hidden;
            let b = from_hidden.shape()[0];
            let l = from_hidden.shape()[1];
            // reference: norm(projected hidden) per slice, THEN add the embed
            // slice, THEN scale the sum
            let proj = from_hidden.reshape(&[b, l, n, w])?;
            let proj = match self.pli_proj_norm.as_ref() {
                Some(wgt) => {
                    let wgt = wgt.as_ref().as_dtype(proj.dtype())?;
                    mlx_rs::fast::rms_norm(&proj, wgt, self.pli_norm_eps)?
                }
                None => proj,
            };
            let emb = from_embed.reshape(&[b, l, n, w])?;
            let split = proj.add(&emb)?;
            let split = split.multiply(&array!(self.pli_input_scale).as_dtype(split.dtype())?)?;
            let mut slices = Vec::with_capacity(n as usize);
            for i in 0..n {
                slices.push(split.index((.., .., i, ..)));
            }
            Some(slices)
        } else {
            None
        };

        let mut intermediates: Vec<Option<KvIntermediate>> =
            (0..self.layers.len()).map(|_| None).collect();
        for i in 0..self.layers.len() {
            let src = self.previous_kvs[i];
            let shared_kv = intermediates[src]
                .as_ref()
                .map(|k| (&k.keys, &k.values, k.pos));
            let (out, kv) = self.layers[i].forward(
                &h,
                pli_slices.as_ref().map(|s| &s[i]),
                shared_kv,
                &mut self.caches[i],
            )?;
            intermediates[i] = kv;
            h = out;
        }
        let h = self.norm.forward(&h)?;
        let mut logits = match &self.embed {
            MaybeEmbed::Quantized(q) => q.as_linear(&h)?,
            MaybeEmbed::Original(e) => e.as_linear(&h)?,
        };
        if let Some(cap) = self.softcap {
            logits = tanh(logits.multiply(array!(1.0 / cap))?)?.multiply(array!(cap))?;
        }
        Ok(logits)
    }

    pub fn reset(&mut self) {
        self.caches = self
            .is_full
            .iter()
            .map(|full| {
                if *full {
                    AttnCache::Full(crate::kv::QuantKvPair::new())
                } else {
                    AttnCache::Roll(RollCache {
                        kv: crate::kv::QuantKvPair::new(),
                        count: 0,
                    })
                }
            })
            .collect();
    }

    /// Materialize every layer's packed cache arrays (once per decode step).
    pub(crate) fn eval_caches(&self) -> Ex<()> {
        for c in &self.caches {
            c.eval()?;
        }
        Ok(())
    }
}
