use mlx_rs::Array;
use susutaku_mlx::kv::QuantKvPair;

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
