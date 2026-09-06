//! Reusable quantized-layer primitives: weight lookup, quantized linear /
//! embedding construction, RMS norm helpers and safetensors loading.
//! No model-architecture context lives here.

use std::collections::HashMap;
use std::path::Path;

use mlx_rs::{Array, array, module::Param, nn, ops::mean_axis, ops::square};

pub(crate) const GROUP_SIZE_FALLBACK: i32 = 64;
pub(crate) const BITS_FALLBACK: i32 = 4;
const NORM_EPS: f32 = 1e-6;
pub(crate) const CONFIG_FILE: &str = "config.json";
const SAFETENSORS_EXT: &str = "safetensors";

pub(crate) type Ex<T> = Result<T, Box<dyn std::error::Error>>;
pub(crate) type MaybeLinear = mlx_rs::quantization::MaybeQuantized<nn::Linear>;
pub(crate) type MaybeEmbed = mlx_rs::quantization::MaybeQuantized<nn::Embedding>;

/// `model_type` from config.json, falling back to the multimodal text_config.
pub(crate) fn model_type(json: &serde_json::Value) -> &str {
    json["model_type"].as_str().unwrap_or_else(|| {
        json["text_config"]["model_type"]
            .as_str()
            .unwrap_or_default()
    })
}

pub(crate) fn req(map: &HashMap<String, Array>, key: &str) -> Ex<Array> {
    map.get(key)
        .cloned()
        .ok_or_else(|| format!("missing weight: {key}").into())
}

pub(crate) fn linear_from_bits(
    map: &HashMap<String, Array>,
    key: &str,
    group_size: i32,
    bits: i32,
) -> Ex<MaybeLinear> {
    let bias = map.get(&format!("{key}.bias")).cloned();
    let scales_key = format!("{key}.scales");
    if map.contains_key(&scales_key) {
        let inner = nn::Linear {
            weight: Param::new(req(map, &format!("{key}.weight"))?),
            bias: Param::new(bias),
        };
        Ok(MaybeLinear::Quantized(nn::QuantizedLinear {
            group_size,
            bits,
            scales: Param::new(req(map, &scales_key)?),
            biases: Param::new(req(map, &format!("{key}.biases"))?),
            inner,
        }))
    } else {
        Ok(MaybeLinear::Original(nn::Linear {
            weight: Param::new(req(map, &format!("{key}.weight"))?),
            bias: Param::new(bias),
        }))
    }
}

pub(crate) fn embed_from_bits(
    map: &HashMap<String, Array>,
    key: &str,
    group_size: i32,
    bits: i32,
) -> Ex<MaybeEmbed> {
    if map.contains_key(&format!("{key}.scales")) {
        Ok(MaybeEmbed::Quantized(nn::QuantizedEmbedding {
            group_size,
            bits,
            scales: Param::new(req(map, &format!("{key}.scales"))?),
            biases: Param::new(req(map, &format!("{key}.biases"))?),
            inner: nn::Embedding {
                weight: Param::new(req(map, &format!("{key}.weight"))?),
            },
        }))
    } else {
        Ok(MaybeEmbed::Original(nn::Embedding {
            weight: Param::new(req(map, &format!("{key}.weight"))?),
        }))
    }
}

pub(crate) fn rms(weight: Array, eps: f32) -> nn::RmsNorm {
    nn::RmsNorm {
        weight: Param::new(weight),
        eps,
    }
}

pub(crate) fn rms_scale(x: &Array, mult: f32) -> Ex<Array> {
    let n = mean_axis(square(x)?, -1, true)?
        .add(array!(NORM_EPS))?
        .sqrt()?
        .reciprocal()?;
    Ok(x.multiply(&n)?.multiply(array!(mult))?)
}

fn strip_prefix(k: &str) -> String {
    k.strip_prefix("language_model.model.")
        .or_else(|| k.strip_prefix("model."))
        .or_else(|| k.strip_prefix("language_model."))
        .unwrap_or(k)
        .to_string()
}

/// All safetensors weights in `dir`, key prefixes stripped.
pub(crate) fn load_map(dir: &Path) -> Ex<HashMap<String, Array>> {
    let mut map = HashMap::new();
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.extension().is_some_and(|e| e == SAFETENSORS_EXT) {
            for (k, v) in Array::load_safetensors(&p)? {
                map.insert(strip_prefix(&k), v);
            }
        }
    }
    Ok(map)
}
