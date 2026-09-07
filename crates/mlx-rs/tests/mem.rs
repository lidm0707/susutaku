use susutaku_mlx::mem::{MEM_BUDGET_BYTES, SAFETY_FREE_FRAC, SAFETY_FREE_MIN};

#[test]
fn budget_leaves_safety_free_space() {
    let installed = *susutaku_mlx::platform::INSTALLED_BYTES;
    let budget = *MEM_BUDGET_BYTES;
    let free = installed - budget;
    let min_free = SAFETY_FREE_MIN;
    assert!(free >= min_free, "free {free} < {min_free}");
    let quarter = (installed as f64 * SAFETY_FREE_FRAC) as usize;
    assert_eq!(free, quarter.max(min_free));
}
