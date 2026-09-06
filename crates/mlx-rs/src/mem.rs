//! GPU memory guard (plan 06): thin bindings to the mlx-c memory API plus a
//! step-budget check. MLX keeps freed device buffers in an internal cache,
//! so resident memory grows even when the live graph is small; the guard
//! caps that cache and aborts generation when the budget is still exceeded.

/// Share of installed RAM always left free (safety margin for the OS,
/// other apps, and mmap headroom).
const SAFETY_FREE_FRAC: f64 = 0.25;
/// The safety margin never shrinks below this, even on small machines.
const SAFETY_FREE_MIN: usize = 16 * GIB;
/// Hard resident-memory budget: installed RAM minus the safety margin
/// (64 GB machine → 48 GiB budget).
pub static MEM_BUDGET_BYTES: LazyLock<usize> = LazyLock::new(budget_bytes);
const GIB: usize = 1024 * 1024 * 1024;

/// The buffer cache is capped low — freed buffers should be reclaimed, not
/// pooled (the Activity Monitor 70-100 GiB was cache + wired, not live KV).
pub const MEM_CACHE_LIMIT_BYTES: usize = 8 * GIB;
/// MLX on macOS wires (pins) buffers for the GPU up to a large default;
/// cap it at the budget so the OS reclaims instead of growing resident size.
pub static MEM_WIRED_LIMIT_BYTES: LazyLock<usize> = LazyLock::new(|| *MEM_BUDGET_BYTES);

use std::sync::LazyLock;

fn budget_bytes() -> usize {
    let installed = *crate::platform::INSTALLED_BYTES;
    let safety = (installed as f64 * SAFETY_FREE_FRAC) as usize;
    installed - safety.max(SAFETY_FREE_MIN)
}

/// Memory log / guard cadence, in decode steps.
pub const MEM_CHECK_EVERY: usize = 16;

unsafe extern "C" {
    fn mlx_get_active_memory(res: *mut usize) -> i32;
    fn mlx_get_cache_memory(res: *mut usize) -> i32;
    fn mlx_get_peak_memory(res: *mut usize) -> i32;
    fn mlx_clear_cache() -> i32;
    fn mlx_set_cache_limit(res: *mut usize, limit: usize) -> i32;
    fn mlx_set_wired_limit(res: *mut usize, limit: usize) -> i32;
}

fn get(f: unsafe extern "C" fn(*mut usize) -> i32) -> usize {
    let mut v = 0usize;
    let status = unsafe { f(&mut v) };
    assert_eq!(status, 0, "mlx memory query failed");
    v
}

pub fn active_bytes() -> usize {
    get(mlx_get_active_memory)
}

pub fn cache_bytes() -> usize {
    get(mlx_get_cache_memory)
}

pub fn peak_bytes() -> usize {
    get(mlx_get_peak_memory)
}

/// Cap MLX's free-buffer cache and the wired (pinned) limit; call once
/// after model load.
pub fn init() {
    let mut old = 0usize;
    let status = unsafe { mlx_set_cache_limit(&mut old, MEM_CACHE_LIMIT_BYTES) };
    assert_eq!(status, 0, "mlx_set_cache_limit failed");
    let status = unsafe { mlx_set_wired_limit(&mut old, *MEM_WIRED_LIMIT_BYTES) };
    assert_eq!(status, 0, "mlx_set_wired_limit failed");
}

pub(crate) fn clear_cache() {
    unsafe { mlx_clear_cache() };
}

/// One-line `(active, cache, peak)` snapshot in GiB, for the periodic log.
pub fn snapshot_gib() -> (f32, f32, f32) {
    const F: f32 = GIB as f32;
    (
        active_bytes() as f32 / F,
        cache_bytes() as f32 / F,
        peak_bytes() as f32 / F,
    )
}

/// One budget check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemVerdict {
    Ok,
    /// Cache was cleared; over budget only by reclaimable buffers.
    Cleared,
    /// Over budget even after clearing live allocations — must abort.
    Over,
}

impl MemVerdict {
    pub fn is_over(self) -> bool {
        matches!(self, MemVerdict::Over)
    }
}

/// One check: if resident memory exceeds the budget, drop the buffer cache
/// first and only report fatal when live allocations alone still overflow.
pub fn check() -> MemVerdict {
    let resident = active_bytes() + cache_bytes();
    if resident <= *MEM_BUDGET_BYTES {
        return MemVerdict::Ok;
    }
    clear_cache();
    if active_bytes() > *MEM_BUDGET_BYTES {
        MemVerdict::Over
    } else {
        MemVerdict::Cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_leaves_safety_free_space() {
        let installed = *crate::platform::INSTALLED_BYTES;
        let budget = *MEM_BUDGET_BYTES;
        let free = installed - budget;
        let min_free = SAFETY_FREE_MIN;
        assert!(free >= min_free, "free {free} < {min_free}");
        let quarter = (installed as f64 * SAFETY_FREE_FRAC) as usize;
        assert_eq!(free, quarter.max(min_free));
    }
}
