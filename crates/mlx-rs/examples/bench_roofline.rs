//! Measures this machine's real MLX/Metal peaks and prints the consts for
//! `src/roofline.rs`. Run: cargo run -p susutaku-mlx --example bench_roofline

use mlx_rs::{
    Array,
    ops::{add, matmul},
    transforms::eval,
};

const ADD_BYTES: usize = 512 * 1024 * 1024; // per f32 array
const ADD_ROUNDS: usize = 10;
const MM_N: i32 = 4096;
const MM_ROUNDS: usize = 10;

fn bandwidth_gbs() -> f64 {
    let a = Array::ones::<f32>(&[ADD_BYTES as i32 / 4]).unwrap();
    let b = Array::ones::<f32>(&[ADD_BYTES as i32 / 4]).unwrap();
    let bytes_per_round = 3.0_f64 * ADD_BYTES as f64; // read a + b, write out
    let mut best = 0.0_f64;
    for _ in 0..ADD_ROUNDS {
        let t = std::time::Instant::now();
        let c = add(&a, &b).unwrap();
        eval(std::slice::from_ref(&c)).unwrap();
        let gbs = bytes_per_round / t.elapsed().as_secs_f64() / 1e9;
        best = best.max(gbs);
    }
    best
}

fn gflops_f16() -> f64 {
    let f32a = Array::ones::<f32>(&[MM_N, MM_N]).unwrap();
    let a = f32a.as_dtype(mlx_rs::Dtype::Float16).unwrap();
    let flops = 2.0_f64 * MM_N as f64 * MM_N as f64 * MM_N as f64;
    let mut best = 0.0_f64;
    for _ in 0..MM_ROUNDS {
        let t = std::time::Instant::now();
        let c = matmul(&a, &a).unwrap();
        eval(std::slice::from_ref(&c)).unwrap();
        let gf = flops / t.elapsed().as_secs_f64() / 1e9;
        best = best.max(gf);
    }
    best
}

fn main() {
    let bw = bandwidth_gbs();
    let gf = gflops_f16();
    println!("measured bandwidth : {bw:.0} GB/s");
    println!("measured f16 matmul: {gf:.0} GFLOP/s");
    println!();
    println!("roofline.rs consts:");
    println!("pub const PEAK_GFLOPS_F16: f64 = {gf:.0};");
    println!("pub const PEAK_BANDWIDTH_GBS: f64 = {bw:.0};");
}
