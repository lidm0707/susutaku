use candle_core::{DType, Tensor};

pub const TOP_K: usize = 40;

/// Sample one token: temperature-scaled top-k on CPU, argmax at temp 0.
pub fn sample(logits: &Tensor, temp: f64) -> Result<u32, String> {
    let logits = logits
        .to_dtype(DType::F32)
        .and_then(|t| t.squeeze(0))
        .map_err(|e| e.to_string())?;
    if temp <= 0.0 {
        return logits
            .argmax(0)
            .and_then(|t| t.to_scalar::<u32>())
            .map_err(|e| e.to_string());
    }
    let mut probs = softmax(&logits)?;
    top_k_mask(&mut probs, TOP_K);
    let total: f32 = probs.iter().sum();
    let mut pick = rand::random::<f32>() * total;
    for (id, &p) in probs.iter().enumerate() {
        pick -= p;
        if pick <= 0.0 {
            return Ok(id as u32);
        }
    }
    probs
        .len()
        .checked_sub(1)
        .map(|i| i as u32)
        .ok_or_else(|| "empty logits".to_string())
}

fn softmax(logits: &Tensor) -> Result<Vec<f32>, String> {
    let max = logits
        .max(0)
        .and_then(|t| t.to_scalar::<f32>())
        .map_err(|e| e.to_string())?;
    let exp: Vec<f32> = logits
        .to_vec1::<f32>()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|&l| (l - max).exp())
        .collect();
    let sum: f32 = exp.iter().sum();
    Ok(exp.iter().map(|&e| e / sum).collect())
}

/// Keep the k highest-probability ids, zero the rest (top-k filter).
fn top_k_mask(probs: &mut [f32], k: usize) {
    if k == 0 || probs.len() <= k {
        return;
    }
    let mut order: Vec<usize> = (0..probs.len()).collect();
    order.sort_unstable_by(|&a, &b| probs[b].total_cmp(&probs[a]));
    for &i in &order[k..] {
        probs[i] = 0.0;
    }
}
