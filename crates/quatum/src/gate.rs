use math::Complex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub dim: usize,
    pub m: [[Complex; 4]; 4],
}

impl Gate {
    pub fn apply(&self, input: &[Complex]) -> Vec<Complex> {
        assert_eq!(input.len(), self.dim);
        let mut out = vec![Complex::ZERO; input.len()];
        for (r, row) in out.iter_mut().enumerate() {
            let mut acc = Complex::ZERO;
            for (c, amp) in input.iter().enumerate() {
                acc = acc + self.m[r][c] * *amp;
            }
            *row = acc;
        }
        out
    }
}

fn m2(a: Complex, b: Complex, c: Complex, d: Complex) -> [[Complex; 4]; 4] {
    let z = Complex::ZERO;
    [[a, b, z, z], [c, d, z, z], [z, z, z, z], [z, z, z, z]]
}

pub struct Gates;

impl Gates {
    pub fn identity() -> Gate {
        Gate {
            dim: 2,
            m: m2(Complex::ONE, Complex::ZERO, Complex::ZERO, Complex::ONE),
        }
    }

    pub fn x() -> Gate {
        Gate {
            dim: 2,
            m: m2(Complex::ZERO, Complex::ONE, Complex::ONE, Complex::ZERO),
        }
    }

    pub fn y() -> Gate {
        Gate {
            dim: 2,
            m: m2(
                Complex::ZERO,
                Complex::new(0.0, -1.0),
                Complex::I,
                Complex::ZERO,
            ),
        }
    }

    pub fn z() -> Gate {
        Gate {
            dim: 2,
            m: m2(
                Complex::ONE,
                Complex::ZERO,
                Complex::ZERO,
                Complex::new(-1.0, 0.0),
            ),
        }
    }

    pub fn hadamard() -> Gate {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        Gate {
            dim: 2,
            m: m2(
                Complex::new(s, 0.0),
                Complex::new(s, 0.0),
                Complex::new(s, 0.0),
                Complex::new(-s, 0.0),
            ),
        }
    }

    pub fn phase(theta: f64) -> Gate {
        Gate {
            dim: 2,
            m: m2(
                Complex::ONE,
                Complex::ZERO,
                Complex::ZERO,
                Complex::from_polar(1.0, theta),
            ),
        }
    }

    pub fn cnot() -> Gate {
        let o = Complex::ONE;
        let z = Complex::ZERO;
        Gate {
            dim: 4,
            m: [[o, z, z, z], [z, o, z, z], [z, z, z, o], [z, z, o, z]],
        }
    }
}
