use susutaku_mlx::platform::{CHIP, INSTALLED_BYTES};

#[test]
fn chip_is_apple_silicon() {
    assert!(CHIP.starts_with("Apple "), "chip: {}", *CHIP);
}

#[test]
fn installed_ram_is_aligned_gib() {
    assert!(*INSTALLED_BYTES >= 8 * 1024 * 1024 * 1024);
    assert_eq!(*INSTALLED_BYTES % (1024 * 1024 * 1024), 0);
}
