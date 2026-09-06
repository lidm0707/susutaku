# Roofline calibration — M5 Max (64 GB)

Measured by `crates/mlx-rs/examples/bench_roofline.rs` (MLX/Metal, 2026-09).

| Metric                | Measured      | Method                                  |
|-----------------------|---------------|-----------------------------------------|
| Memory bandwidth      | **518 GB/s**  | 512 MiB f32 elementwise add, best of 10 |
| f16 matmul throughput | **14778 GFLOP/s** | 4096³ f16 matmul, best of 10        |

These values are baked into `crates/mlx-rs/src/roofline.rs`
(`PEAK_BANDWIDTH_GBS`, `PEAK_GFLOPS_F16`). Rerun the example after macOS/MLX
updates and refresh the consts.

## What it says about decode

- Decode is a GEMV: at 27B Q4 (~13.6 GiB weights/step) the bandwidth bound is
  ~27 tok/s ceiling — matches the ~25.5 tok/s measured in plan 05. We are at
  ~93% of the bandwidth roofline; no decode-side scheduling trick (n-gram
  drafting, tree verify) can beat it. Wins must come from moving fewer bytes
  (KV quant — plan 06, KV eviction — plan 08-deferred) or fewer steps.
- Prefill is a GEMM: compute-bound at ~14.8 TFLOP/s — this is where the M5 Max
  shines vs older chips; long prompts amortize well.
