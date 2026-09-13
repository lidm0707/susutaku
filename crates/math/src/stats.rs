pub fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    Some(sum / values.len() as f64)
}

pub fn variance(values: &[f64]) -> Option<f64> {
    let mu = mean(values)?;
    if values.len() < 2 {
        return Some(0.0);
    }
    let sq_sum: f64 = values.iter().map(|v| (v - mu) * (v - mu)).sum();
    Some(sq_sum / (values.len() - 1) as f64)
}

pub fn min(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::min)
}

pub fn max(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::max)
}
