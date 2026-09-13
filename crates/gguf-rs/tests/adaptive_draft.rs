use gguf_rs::model::{DRAFT_MIN_ALPHA, DRAFT_STEPS, adaptive_draft_len};

#[test]
fn draft_len_scales_with_forecast_alpha() {
    // Peaked target (α=1): full window.
    assert_eq!(adaptive_draft_len(1.0), DRAFT_STEPS);
    // Middling α: proportional window, never below one.
    assert_eq!(adaptive_draft_len(0.5), DRAFT_STEPS / 2);
    // Gate floor: smallest window still allowed.
    assert_eq!(adaptive_draft_len(DRAFT_MIN_ALPHA), 2);
    // Over-range α clamps to the window cap.
    assert_eq!(adaptive_draft_len(9.0), DRAFT_STEPS);
}

#[test]
fn gate_floor_never_allows_zero() {
    for i in 0..=100u32 {
        let a = i as f32 / 100.0;
        if a >= DRAFT_MIN_ALPHA {
            assert!(adaptive_draft_len(a) >= 1);
        }
    }
}
