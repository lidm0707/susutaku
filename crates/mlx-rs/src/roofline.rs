//! Analytic roofline for engine go/no-go decisions (plan 09).
//!
//! Peaks are MEASURED on the target machine by `examples/bench_roofline.rs`
//! (MLX microbench), not vendor-table guesses — katgpt's table stops at M4
//! with numbers that rank M4 Pro below M2 Pro.

/// Peak f16 matmul throughput, GFLOP/s (measured via bench_roofline,
/// M5 Max, 2026-09).
pub const PEAK_GFLOPS_F16: f64 = 14778.0;
/// Peak memory bandwidth, GB/s (measured via bench_roofline, M5 Max).
pub const PEAK_BANDWIDTH_GBS: f64 = 518.0;
/// Per-op Metal dispatch overhead, µs (conservative; not measured).
pub const LAUNCH_OVERHEAD_US: f64 = 5.0;

/// Weight/activation encodings relevant to decode cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
    F32,
    F16,
    /// 4-bit weights: 0.5 B/elem, plus a dequant pass on the GPU.
    Q4,
}

impl Dtype {
    pub fn elem_size(self) -> f64 {
        match self {
            Self::F32 => 4.0,
            Self::F16 => 2.0,
            Self::Q4 => 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Compute,
    Memory,
}

#[derive(Debug, Clone, Copy)]
pub struct Cost {
    pub runtime_ms: f64,
    pub bound: Bound,
}

/// `runtime = max(compute, memory)`; launch overhead folded in as a floor.
fn estimate(flops: f64, bytes: f64) -> Cost {
    const {
        assert!(
            PEAK_GFLOPS_F16 > 0.0 && PEAK_BANDWIDTH_GBS > 0.0,
            "peaks not calibrated — run examples/bench_roofline.rs and fill the consts"
        );
    }
    let compute_ms = flops / (PEAK_GFLOPS_F16 * 1e6);
    let memory_ms = bytes / (PEAK_BANDWIDTH_GBS * 1e6);
    let floor_ms = LAUNCH_OVERHEAD_US / 1000.0;
    Cost {
        runtime_ms: compute_ms.max(memory_ms).max(floor_ms),
        bound: if compute_ms >= memory_ms {
            Bound::Compute
        } else {
            Bound::Memory
        },
    }
}

/// Decode-step matvec: weights (m × k) read once, activations negligible.
/// FLOPs = 2·m·k; bytes ≈ weight bytes (streaming model).
pub fn gemv_cost(m: usize, k: usize, dtype: Dtype) -> Cost {
    let (m, k) = (m as f64, k as f64);
    estimate(2.0 * m * k, m * k * dtype.elem_size())
}

/// Prefill/train matmul (m × k) × (k × n).
pub fn gemm_cost(m: usize, n: usize, k: usize, dtype: Dtype) -> Cost {
    let (m, n, k) = (m as f64, n as f64, k as f64);
    let bytes = (m * k + k * n + m * n) * dtype.elem_size();
    estimate(2.0 * m * n * k, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: usize = 4096;
    const K: usize = 4096;
    #[test]
    fn decode_gemv_is_memory_bound() {
        // 27B-scale hidden size: 32 MiB Q4 weights moved vs 268 MFLOP
        let cost = gemv_cost(8192, 8192, Dtype::Q4);
        assert_eq!(cost.bound, Bound::Memory);
    }

    #[test]
    fn prefill_gemm_is_compute_bound() {
        let cost = gemm_cost(2048, 2048, 8192, Dtype::F16);
        assert_eq!(cost.bound, Bound::Compute);
    }

    #[test]
    fn q4_gemev_faster_than_f16_by_elem_size() {
        let q4 = gemv_cost(M, K, Dtype::Q4);
        let f16 = gemv_cost(M, K, Dtype::F16);
        assert!((f16.runtime_ms / q4.runtime_ms - 4.0).abs() < 1e-9);
    }
}
