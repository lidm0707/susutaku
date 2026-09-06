//! Qwen3.8 / qwen3_5 (MLX, 4-bit): hybrid gated-delta-net + full attention.

use std::collections::HashMap;

use mlx_rs::{
    Array, Dtype, array,
    builder::Builder,
    fast::{ScaledDotProductAttentionMask, rms_norm, scaled_dot_product_attention},
    module::{Module, Param},
    nn,
    ops::indexing::IndexOp,
    ops::{concatenate_axis, conv1d, exp, repeat_axis, sigmoid, stack, sum_axis, zeros},
};

use crate::quant::{
    BITS_FALLBACK, Ex, GROUP_SIZE_FALLBACK, MaybeEmbed, MaybeLinear, embed_from_bits,
    linear_from_bits, req, rms, rms_scale,
};

const DEFAULT_ROPE_THETA: f32 = 10_000_000.0;
const DEFAULT_EPS: f32 = 1e-6;
const SUPPORTED_MODEL_TYPE: &str = "qwen3_5";
const LM_HEAD_KEY: &str = "lm_head.weight";

#[derive(Debug)]
struct Config {
    layers: i32,
    heads: i32,
    kv_heads: i32,
    head_dim: i32,
    eps: f32,
    theta: f32,
    rot_dims: i32,
    tie: bool,
    group_size: i32,
    bits: i32,
    num_v: i32,
    num_k: i32,
    dk: i32,
    dv: i32,
    conv_k: i32,
    is_linear: Vec<bool>,
}

impl Config {
    pub(crate) fn parse(json: &serde_json::Value) -> Ex<Self> {
        let t = if json.get("text_config").is_some_and(|t| !t.is_null()) {
            &json["text_config"]
        } else {
            json
        };
        let model_type = json["model_type"]
            .as_str()
            .unwrap_or_else(|| t["model_type"].as_str().unwrap_or_default());
        if model_type != SUPPORTED_MODEL_TYPE {
            return Err(format!(
                "unsupported model_type `{model_type}` (engine only supports {SUPPORTED_MODEL_TYPE})"
            )
            .into());
        }
        let v = |j: &serde_json::Value, k: &str| j[k].as_i64().unwrap_or_default() as i32;
        let head_dim = t["head_dim"]
            .as_i64()
            .map(|d| d as i32)
            .unwrap_or(v(t, "hidden_size") / v(t, "num_attention_heads"));
        let rope_params = &t["rope_parameters"];
        let theta = rope_params["rope_theta"]
            .as_f64()
            .unwrap_or(DEFAULT_ROPE_THETA as f64) as f32;
        let partial = rope_params["partial_rotary_factor"]
            .as_f64()
            .unwrap_or(0.25) as f32;
        let interval = t["full_attention_interval"].as_i64().unwrap_or(4) as usize;
        let n = v(t, "num_hidden_layers");
        let is_linear = (0..n).map(|i| (i + 1) % interval as i32 != 0).collect();
        Ok(Self {
            layers: n,
            heads: v(t, "num_attention_heads"),
            kv_heads: v(t, "num_key_value_heads"),
            head_dim,
            eps: t["rms_norm_eps"].as_f64().unwrap_or(DEFAULT_EPS as f64) as f32,
            theta,
            rot_dims: (head_dim as f32 * partial) as i32,
            tie: t["tie_word_embeddings"].as_bool().unwrap_or(false),
            group_size: json["quantization"]["group_size"]
                .as_i64()
                .unwrap_or(GROUP_SIZE_FALLBACK as i64) as i32,
            bits: json["quantization"]["bits"]
                .as_i64()
                .unwrap_or(BITS_FALLBACK as i64) as i32,
            num_v: v(t, "linear_num_value_heads"),
            num_k: v(t, "linear_num_key_heads"),
            dk: v(t, "linear_key_head_dim"),
            dv: v(t, "linear_value_head_dim"),
            conv_k: v(t, "linear_conv_kernel_dim"),
            is_linear,
        })
    }

    fn linear_from(&self, map: &HashMap<String, Array>, key: &str) -> Ex<MaybeLinear> {
        linear_from_bits(map, key, self.group_size, self.bits)
    }

    fn embed_from(&self, map: &HashMap<String, Array>, key: &str) -> Ex<MaybeEmbed> {
        embed_from_bits(map, key, self.group_size, self.bits)
    }
}

// ---------- full attention (gated, partial rope) ----------

struct Attention {
    q_proj: MaybeLinear,
    k_proj: MaybeLinear,
    v_proj: MaybeLinear,
    o_proj: MaybeLinear,
    q_norm: nn::RmsNorm,
    k_norm: nn::RmsNorm,
    rope: nn::Rope,
    heads: i32,
    kv_heads: i32,
    scale: f32,
}

struct AttnOutput {
    out: Array,
    cache: (Array, Array),
}

impl Attention {
    fn forward(
        &mut self,
        x: &Array,
        mask: Option<ScaledDotProductAttentionMask<'_>>,
        cache: Option<(&Array, &Array)>,
        rope_pos: i32,
    ) -> Ex<AttnOutput> {
        let b = x.shape()[0];
        let l = x.shape()[1];

        let qg = self.q_proj.forward(x)?;
        let qg = qg.reshape(&[b, l, self.heads, -1])?;
        let q = qg.index((.., .., .., 0..256.max(qg.shape()[3] / 2)));
        let gate = qg.index((.., .., .., qg.shape()[3] / 2..));
        let gate = gate.reshape(&[b, l, -1])?;

        let k = self
            .k_proj
            .forward(x)?
            .reshape(&[b, l, self.kv_heads, -1])?;
        let v = self
            .v_proj
            .forward(x)?
            .reshape(&[b, l, self.kv_heads, -1])?;

        let mut q = self.q_norm.forward(&q)?.transpose_axes(&[0, 2, 1, 3])?;
        let mut k = self.k_norm.forward(&k)?.transpose_axes(&[0, 2, 1, 3])?;
        let v = v.transpose_axes(&[0, 2, 1, 3])?;

        let (k, v) = match cache {
            Some((kc, vc)) => {
                q = self.rope.forward((&q, rope_pos))?;
                k = self.rope.forward((&k, rope_pos))?;
                (
                    concatenate_axis(&[kc, &k], 2)?,
                    concatenate_axis(&[vc, &v], 2)?,
                )
            }
            None => {
                q = self.rope.forward(&q)?;
                k = self.rope.forward(&k)?;
                (k, v)
            }
        };

        let out = scaled_dot_product_attention(q, &k, &v, self.scale, mask)?;
        let out = out.transpose_axes(&[0, 2, 1, 3])?.reshape(&[b, l, -1])?;
        let out = out.multiply(&sigmoid(gate)?)?;
        Ok(AttnOutput {
            out: self.o_proj.forward(&out)?,
            cache: (k, v),
        })
    }
}

// ---------- gated delta net (linear attention) ----------

struct Gdn {
    in_proj_qkv: MaybeLinear,
    in_proj_z: MaybeLinear,
    in_proj_a: MaybeLinear,
    in_proj_b: MaybeLinear,
    out_proj: MaybeLinear,
    conv_w: Param<Array>,
    a_log: Param<Array>,
    dt_bias: Param<Array>,
    norm: nn::RmsNorm,
    num_v: i32,
    num_k: i32,
    dk: i32,
    dv: i32,
    conv_k: i32,
}

struct GdnOutput {
    out: Array,
    conv_state: Array,
    state: Array,
}

impl Gdn {
    fn forward(&mut self, x: &Array, cache: Option<(&Array, &Array)>) -> Ex<GdnOutput> {
        let b = x.shape()[0];
        let s = x.shape()[1];
        let key_dim = self.dk * self.num_k;
        let value_dim = self.dv * self.num_v;
        let conv_dim = key_dim * 2 + value_dim;

        let qkv = self.in_proj_qkv.forward(x)?;
        let z = self
            .in_proj_z
            .forward(x)?
            .reshape(&[b, s, self.num_v, self.dv])?;
        let a = self.in_proj_a.forward(x)?;
        let beta_in = self.in_proj_b.forward(x)?;

        let (prev_conv, ssm_in) = match cache {
            Some((c, st)) => (c.clone(), Some(st.clone())),
            None => (
                zeros::<f32>(&[b, self.conv_k - 1, conv_dim])?.as_dtype(x.dtype())?,
                None,
            ),
        };

        let conv_in = concatenate_axis(&[&prev_conv, &qkv], 1)?;
        let conv_state = conv_in.index((.., (conv_in.shape()[1] - (self.conv_k - 1)).., ..));
        let conv_out = nn::silu(conv1d(&conv_in, &self.conv_w, 1, 0, 1, conv_dim)?)?;

        let q = conv_out
            .index((.., .., 0..key_dim))
            .reshape(&[b, s, self.num_k, self.dk])?;
        let k = conv_out
            .index((.., .., key_dim..(2 * key_dim)))
            .reshape(&[b, s, self.num_k, self.dk])?;
        let v = conv_out
            .index((.., .., (2 * key_dim)..(2 * key_dim + value_dim)))
            .reshape(&[b, s, self.num_v, self.dv])?;

        let inv = (self.dk as f32).powf(-0.5);
        let q = rms_scale(&q, inv * inv)?;
        let k = rms_scale(&k, inv)?;

        // g = exp(-exp(A_log) * softplus(a + dt_bias)), beta = sigmoid(b), in f32
        let a32 = a.as_dtype(Dtype::Float32)?;
        let gate_g = exp(exp(self.a_log.as_dtype(Dtype::Float32)?)?
            .multiply(array!(-1.0))?
            .multiply(&nn::softplus(
                a32.add(&self.dt_bias.as_dtype(Dtype::Float32)?)?,
            )?)?)?;
        let beta = sigmoid(beta_in.as_dtype(Dtype::Float32)?)?;

        let mut state = match ssm_in {
            Some(st) => st,
            None => zeros::<f32>(&[b, self.num_v, self.dv, self.dk])?,
        };
        let rep = self.num_v / self.num_k;
        let mut ys: Vec<Array> = Vec::with_capacity(s as usize);
        for t in 0..s {
            let qt = repeat_axis::<u32>(q.index((.., t, ..)), rep, 1)?.as_dtype(Dtype::Float32)?;
            let kt = repeat_axis::<u32>(k.index((.., t, ..)), rep, 1)?.as_dtype(Dtype::Float32)?;
            let vt = v.index((.., t, ..)).as_dtype(Dtype::Float32)?;
            let gt = gate_g.index((.., t, ..)).reshape(&[b, self.num_v, 1, 1])?;
            let bt = beta.index((.., t, ..)).reshape(&[b, self.num_v, 1])?;

            let decayed = state.multiply(&gt)?;
            let kt2 = kt.reshape(&[b, self.num_v, 1, self.dk])?;
            let qt2 = qt.reshape(&[b, self.num_v, 1, self.dk])?;
            let kv_mem = sum_axis(decayed.multiply(&kt2)?, -1, false)?;
            let delta = vt.subtract(&kv_mem)?.multiply(&bt)?;
            let state_new =
                decayed.add(&kt2.multiply(&delta.reshape(&[b, self.num_v, self.dv, 1])?)?)?;
            let y = sum_axis(state_new.multiply(&qt2)?, -1, false)?.as_dtype(x.dtype())?;
            ys.push(y);
            state = state_new;
        }
        let y = stack(&ys.iter().collect::<Vec<_>>())?.transpose_axes(&[1, 0, 2, 3])?;

        // gated rmsnorm: rms(y) * silu(z)
        let normed = rms_norm(&y, &self.norm.weight, self.norm.eps)?;
        let gated = normed
            .multiply(&nn::silu(z)?)?
            .reshape(&[b, s, self.num_v * self.dv])?;
        Ok(GdnOutput {
            out: self.out_proj.forward(&gated)?,
            conv_state,
            state,
        })
    }
}

// ---------- blocks & model ----------

struct Mlp {
    gate_proj: MaybeLinear,
    up_proj: MaybeLinear,
    down_proj: MaybeLinear,
}

impl Mlp {
    fn forward(&mut self, x: &Array) -> Ex<Array> {
        let gate = nn::silu(self.gate_proj.forward(x)?)?;
        let up = self.up_proj.forward(x)?;
        Ok(self.down_proj.forward(&gate.multiply(&up)?)?)
    }
}

enum BlockAttn {
    Linear(Gdn),
    Full(Attention),
}

pub(crate) enum Cache {
    Linear(Option<(Array, Array)>),
    Full(crate::kv::QuantKvPair),
}

impl Cache {
    pub(crate) fn truncate(&mut self, kept: usize) {
        match self {
            Cache::Full(c) => c.truncate(kept),
            Cache::Linear(_) => {}
        }
    }

    /// Sink+recent eviction of stored keys (absolute positions preserved).
    pub(crate) fn compact(&mut self, sink: usize, recent: usize) -> Ex<()> {
        match self {
            Cache::Full(c) => c.compact(sink, recent),
            Cache::Linear(_) => Ok(()),
        }
    }
}

struct Block {
    attn: BlockAttn,
    ln1: nn::RmsNorm,
    ln2: nn::RmsNorm,
    mlp: Mlp,
}

impl Block {
    fn forward(&mut self, x: &Array, cache: &mut Cache) -> Ex<Array> {
        let h = self.ln1.forward(x)?;
        let r = match (&mut self.attn, cache) {
            (BlockAttn::Linear(g), Cache::Linear(c)) => {
                let out = match c.as_ref() {
                    Some((cv, st)) => g.forward(&h, Some((cv, st)))?,
                    None => g.forward(&h, None)?,
                };
                *c = Some((out.conv_state, out.state));
                out.out
            }
            (BlockAttn::Full(a), Cache::Full(c)) => {
                // keys include the cache, so the mask must span cached + new
                let cached = c.k.len();
                let l = x.shape()[1] as usize;
                let mask: Option<Array> = if l > 1 || cached > 0 {
                    let total = cached + l;
                    let mut data = Vec::with_capacity(l * total);
                    for i in 0..l {
                        for j in 0..total {
                            let visible = j < cached || j - cached <= i;
                            data.push(if visible { 0.0 } else { f32::NEG_INFINITY });
                        }
                    }
                    Some(Array::from_slice(&data, &[l as i32, total as i32]).as_dtype(x.dtype())?)
                } else {
                    None
                };
                let entry = c.read()?;
                let pos = c.pos;
                let out = a.forward(
                    &h,
                    mask.as_ref().map(Into::into),
                    entry.as_ref().map(|(k, v)| (k, v)),
                    pos,
                )?;
                // append only the new positions — old chunks are never
                // re-quantized, so error does not compound
                let (k_all, v_all) = out.cache;
                let l_i = l as i32;
                let k_new = k_all.index((.., .., (k_all.shape()[2] - l_i)..));
                let v_new = v_all.index((.., .., (v_all.shape()[2] - l_i)..));
                c.append(&k_new, &v_new)?;
                out.out
            }
            _ => return Err("cache kind mismatch".into()),
        };
        let h2 = x.add(&r)?;
        Ok(h2.add(&self.mlp.forward(&self.ln2.forward(&h2)?)?)?)
    }
}

pub(crate) struct Qwen35 {
    embed_tokens: MaybeEmbed,
    blocks: Vec<Block>,
    norm: nn::RmsNorm,
    lm_head: MaybeLinear,
    is_linear: Vec<bool>,
    pub(crate) caches: Vec<Cache>,
}

impl Qwen35 {
    pub(crate) fn build(json: &serde_json::Value, map: &HashMap<String, Array>) -> Ex<Self> {
        let cfg = Config::parse(json)?;
        let blocks = (0..cfg.layers as usize)
            .map(|i| {
                let p = format!("layers.{i}.");
                let attn = if cfg.is_linear[i] {
                    let lp = format!("{p}linear_attn.");
                    BlockAttn::Linear(Gdn {
                        in_proj_qkv: cfg.linear_from(map, &format!("{lp}in_proj_qkv"))?,
                        in_proj_z: cfg.linear_from(map, &format!("{lp}in_proj_z"))?,
                        in_proj_a: cfg.linear_from(map, &format!("{lp}in_proj_a"))?,
                        in_proj_b: cfg.linear_from(map, &format!("{lp}in_proj_b"))?,
                        out_proj: cfg.linear_from(map, &format!("{lp}out_proj"))?,
                        conv_w: Param::new(req(map, &format!("{lp}conv1d.weight"))?),
                        a_log: Param::new(req(map, &format!("{lp}A_log"))?),
                        dt_bias: Param::new(req(map, &format!("{lp}dt_bias"))?),
                        norm: rms(req(map, &format!("{lp}norm.weight"))?, cfg.eps),
                        num_v: cfg.num_v,
                        num_k: cfg.num_k,
                        dk: cfg.dk,
                        dv: cfg.dv,
                        conv_k: cfg.conv_k,
                    })
                } else {
                    let ap = format!("{p}self_attn.");
                    BlockAttn::Full(Attention {
                        q_proj: cfg.linear_from(map, &format!("{ap}q_proj"))?,
                        k_proj: cfg.linear_from(map, &format!("{ap}k_proj"))?,
                        v_proj: cfg.linear_from(map, &format!("{ap}v_proj"))?,
                        o_proj: cfg.linear_from(map, &format!("{ap}o_proj"))?,
                        q_norm: rms(req(map, &format!("{ap}q_norm.weight"))?, cfg.eps),
                        k_norm: rms(req(map, &format!("{ap}k_norm.weight"))?, cfg.eps),
                        rope: nn::RopeBuilder::new(cfg.rot_dims)
                            .traditional(false)
                            .base(cfg.theta)
                            .build()?,
                        heads: cfg.heads,
                        kv_heads: cfg.kv_heads,
                        scale: (cfg.head_dim as f32).powf(-0.5),
                    })
                };
                Ok(Block {
                    attn,
                    ln1: rms(req(map, &format!("{p}input_layernorm.weight"))?, cfg.eps),
                    ln2: rms(
                        req(map, &format!("{p}post_attention_layernorm.weight"))?,
                        cfg.eps,
                    ),
                    mlp: Mlp {
                        gate_proj: cfg.linear_from(map, &format!("{p}mlp.gate_proj"))?,
                        up_proj: cfg.linear_from(map, &format!("{p}mlp.up_proj"))?,
                        down_proj: cfg.linear_from(map, &format!("{p}mlp.down_proj"))?,
                    },
                })
            })
            .collect::<Ex<Vec<_>>>()?;

        let lm_head = if map.contains_key(LM_HEAD_KEY) || map.contains_key("lm_head.scales") {
            cfg.linear_from(map, "lm_head")?
        } else if cfg.tie {
            cfg.linear_from(map, "embed_tokens")?
        } else {
            return Err("no lm_head and embeddings not tied".into());
        };

        let caches = cfg
            .is_linear
            .iter()
            .map(|lin| {
                if *lin {
                    Cache::Linear(None)
                } else {
                    Cache::Full(crate::kv::QuantKvPair::new())
                }
            })
            .collect();

        Ok(Self {
            embed_tokens: cfg.embed_from(map, "embed_tokens")?,
            blocks,
            norm: rms(req(map, "norm.weight")?, cfg.eps),
            lm_head,
            is_linear: cfg.is_linear.clone(),
            caches,
        })
    }

    pub(crate) fn forward(&mut self, tokens: &Array) -> Ex<Array> {
        let mut h = self.embed_tokens.forward(tokens)?;
        for (block, cache) in self.blocks.iter_mut().zip(self.caches.iter_mut()) {
            h = block.forward(&h, cache)?;
        }
        self.lm_head
            .forward(&self.norm.forward(&h)?)
            .map_err(Into::into)
    }

    /// Materialize every layer's packed cache arrays (once per decode step).
    pub(crate) fn eval_caches(&self) -> Ex<()> {
        for c in &self.caches {
            if let Cache::Full(kv) = c {
                kv.eval()?;
            }
        }
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.caches = self
            .is_linear
            .iter()
            .map(|lin| {
                if *lin {
                    Cache::Linear(None)
                } else {
                    Cache::Full(crate::kv::QuantKvPair::new())
                }
            })
            .collect();
    }
}
