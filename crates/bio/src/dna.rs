use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    A,
    C,
    G,
    T,
}

impl fmt::Display for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = match self {
            Base::A => 'A',
            Base::C => 'C',
            Base::G => 'G',
            Base::T => 'T',
        };
        write!(f, "{c}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RnaBase {
    A,
    C,
    G,
    U,
}

impl fmt::Display for RnaBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = match self {
            RnaBase::A => 'A',
            RnaBase::C => 'C',
            RnaBase::G => 'G',
            RnaBase::U => 'U',
        };
        write!(f, "{c}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AminoAcid {
    Phe,
    Leu,
    Ile,
    Met,
    Val,
    Ser,
    Pro,
    Thr,
    Ala,
    Tyr,
    His,
    Gln,
    Asn,
    Lys,
    Asp,
    Glu,
    Cys,
    Trp,
    Arg,
    Gly,
    Stop,
}

impl AminoAcid {
    pub fn code(self) -> char {
        match self {
            AminoAcid::Phe => 'F',
            AminoAcid::Leu => 'L',
            AminoAcid::Ile => 'I',
            AminoAcid::Met => 'M',
            AminoAcid::Val => 'V',
            AminoAcid::Ser => 'S',
            AminoAcid::Pro => 'P',
            AminoAcid::Thr => 'T',
            AminoAcid::Ala => 'A',
            AminoAcid::Tyr => 'Y',
            AminoAcid::His => 'H',
            AminoAcid::Gln => 'Q',
            AminoAcid::Asn => 'N',
            AminoAcid::Lys => 'K',
            AminoAcid::Asp => 'D',
            AminoAcid::Glu => 'E',
            AminoAcid::Cys => 'C',
            AminoAcid::Trp => 'W',
            AminoAcid::Arg => 'R',
            AminoAcid::Gly => 'G',
            AminoAcid::Stop => '*',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codon(pub [RnaBase; 3]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dna {
    bases: Vec<Base>,
}

impl Dna {
    pub fn parse(s: &str) -> Option<Self> {
        let mut bases = Vec::with_capacity(s.len());
        for c in s.chars() {
            let b = match c {
                'A' | 'a' => Base::A,
                'C' | 'c' => Base::C,
                'G' | 'g' => Base::G,
                'T' | 't' => Base::T,
                _ => return None,
            };
            bases.push(b);
        }
        Some(Self { bases })
    }

    pub fn bases(&self) -> &[Base] {
        &self.bases
    }

    pub fn len(&self) -> usize {
        self.bases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bases.is_empty()
    }

    pub fn complement_base(b: Base) -> Base {
        match b {
            Base::A => Base::T,
            Base::T => Base::A,
            Base::C => Base::G,
            Base::G => Base::C,
        }
    }

    pub fn complement(&self) -> Self {
        Self {
            bases: self
                .bases
                .iter()
                .rev()
                .map(|&b| Self::complement_base(b))
                .collect(),
        }
    }

    pub fn transcribe(&self) -> Vec<RnaBase> {
        self.bases
            .iter()
            .map(|&b| match b {
                Base::A => RnaBase::A,
                Base::C => RnaBase::C,
                Base::G => RnaBase::G,
                Base::T => RnaBase::U,
            })
            .collect()
    }

    pub fn gc_content(&self) -> f64 {
        if self.bases.is_empty() {
            return 0.0;
        }
        let gc = self
            .bases
            .iter()
            .filter(|b| matches!(b, Base::C | Base::G))
            .count();
        gc as f64 / self.bases.len() as f64
    }

    pub fn translate(&self) -> Vec<AminoAcid> {
        let rna = self.transcribe();
        rna.chunks_exact(3)
            .map(|c| translate_codon(Codon([c[0], c[1], c[2]])))
            .take_while(|aa| *aa != AminoAcid::Stop)
            .collect()
    }
}

pub fn translate_codon(c: Codon) -> AminoAcid {
    use AminoAcid::*;
    use RnaBase as R;
    match c {
        Codon([R::U, R::U, R::U | R::C]) => Phe,
        Codon([R::U, R::U, R::A | R::G]) | Codon([R::C, R::U, _]) => Leu,
        Codon([R::A, R::U, R::G]) => Met,
        Codon([R::A, R::U, R::U | R::C | R::A]) => Ile,
        Codon([R::G, R::U, _]) => Val,
        Codon([R::U, R::C, _]) | Codon([R::A, R::G, R::U | R::C]) => Ser,
        Codon([R::C, R::C, _]) => Pro,
        Codon([R::A, R::C, _]) => Thr,
        Codon([R::G, R::C, _]) => Ala,
        Codon([R::U, R::A, R::U | R::C]) => Tyr,
        Codon([R::C, R::A, R::U | R::C]) => His,
        Codon([R::C, R::A, R::A | R::G]) => Gln,
        Codon([R::A, R::A, R::U | R::C]) => Asn,
        Codon([R::A, R::A, R::A | R::G]) => Lys,
        Codon([R::G, R::A, R::U | R::C]) => Asp,
        Codon([R::G, R::A, R::A | R::G]) => Glu,
        Codon([R::U, R::G, R::U | R::C]) => Cys,
        Codon([R::U, R::G, R::G]) => Trp,
        Codon([R::A, R::G, R::A | R::G]) | Codon([R::C, R::G, _]) => Arg,
        Codon([R::G, R::G, _]) => Gly,
        Codon([R::U, R::A, R::A | R::G]) | Codon([R::U, R::G, R::A]) => Stop,
    }
}
