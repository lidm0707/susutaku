use math::Complex;

pub type QubitState = StateVector;

/// Probability distribution over computational basis outcomes.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub probs: Vec<f64>,
}

impl Measurement {
    pub fn most_likely(&self) -> usize {
        self.probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateVector {
    pub amps: Vec<Complex>,
}

impl StateVector {
    pub fn basis(n_qubits: usize, index: usize) -> Self {
        assert!(index < 1 << n_qubits);
        let mut amps = vec![Complex::ZERO; 1 << n_qubits];
        amps[index] = Complex::ONE;
        Self { amps }
    }

    pub fn n_qubits(&self) -> usize {
        self.amps.len().trailing_zeros() as usize
    }

    pub fn measure(&self) -> Measurement {
        Measurement {
            probs: self.amps.iter().map(|a| a.norm_sqr()).collect(),
        }
    }

    pub fn normalize(&mut self) {
        let norm: f64 = self.amps.iter().map(|a| a.norm_sqr()).sum::<f64>().sqrt();
        if norm == 0.0 {
            return;
        }
        for a in &mut self.amps {
            *a = *a / norm;
        }
    }

    /// Kronecker product: self ⊗ other
    pub fn tensor(&self, other: &Self) -> Self {
        let mut amps = Vec::with_capacity(self.amps.len() * other.amps.len());
        for a in &self.amps {
            for b in &other.amps {
                amps.push(*a * *b);
            }
        }
        Self { amps }
    }
}
