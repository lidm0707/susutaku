use susutaku_mlx::roofline::{gemm_cost, gemv_cost, Bound, Dtype};

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
