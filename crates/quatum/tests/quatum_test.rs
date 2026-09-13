use math::Complex;
use quatum::{Gates, QubitState, StateVector};

const EPS: f64 = 1e-9;

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "{a} != {b}");
}

#[test]
fn basis_state_probs() {
    let s = StateVector::basis(1, 0);
    let m = s.measure();
    approx(m.probs[0], 1.0);
    approx(m.probs[1], 0.0);
}

#[test]
fn hadamard_superposition() {
    let h = Gates::hadamard();
    let out = h.apply(&StateVector::basis(1, 0).amps);
    let s = QubitState { amps: out };
    let m = s.measure();
    approx(m.probs[0], 0.5);
    approx(m.probs[1], 0.5);
}

#[test]
fn x_flips_bit() {
    let out = Gates::x().apply(&StateVector::basis(1, 0).amps);
    approx(out[1].re, 1.0);
    let back = Gates::x().apply(&out);
    approx(back[0].re, 1.0);
}

#[test]
fn z_phase_kickback() {
    let h = Gates::hadamard();
    let sup = h.apply(&StateVector::basis(1, 0).amps);
    let out = Gates::z().apply(&sup);
    approx(out[0].norm_sqr(), 0.5);
    approx(out[1].norm_sqr(), 0.5);
    approx(out[1].re, -std::f64::consts::FRAC_1_SQRT_2);
}

#[test]
fn cnot_entangles() {
    // |00> -> H on first qubit (as |0>+|1> on qubit0 tensor |0>) -> CNOT -> Bell
    let h = Gates::hadamard();
    let sup = h.apply(&StateVector::basis(1, 0).amps);
    let two = StateVector { amps: sup }.tensor(&StateVector::basis(1, 0));
    let bell = Gates::cnot().apply(&two.amps);
    let s = QubitState { amps: bell };
    let m = s.measure();
    approx(m.probs[0], 0.5);
    approx(m.probs[1], 0.0);
    approx(m.probs[2], 0.0);
    approx(m.probs[3], 0.5);
}

#[test]
fn most_likely_picks_peak() {
    let m = StateVector::basis(2, 2).measure();
    assert_eq!(m.most_likely(), 2);
}

#[test]
fn normalize_and_tensor_dims() {
    let mut s = QubitState {
        amps: vec![Complex::new(2.0, 0.0), Complex::ZERO],
    };
    s.normalize();
    approx(s.amps[0].re, 1.0);
    assert_eq!(
        StateVector::basis(1, 0)
            .tensor(&StateVector::basis(1, 1))
            .amps
            .len(),
        4
    );
}
