use crate::element::Element;
use crate::formula::{parse_formula, FormulaComponent};

pub fn molar_mass(components: &[FormulaComponent]) -> f64 {
    components
        .iter()
        .map(|c| match c {
            FormulaComponent::Atom(el, n) => el.atomic_mass() * *n as f64,
            FormulaComponent::Group(inner, n) => molar_mass(inner) * *n as f64,
        })
        .sum()
}

pub fn molar_mass_of(formula: &str) -> Option<f64> {
    Some(molar_mass(&parse_formula(formula)?))
}

pub fn moles_from_grams(mass_g: f64, molar_mass_g_per_mol: f64) -> f64 {
    mass_g / molar_mass_g_per_mol
}

pub fn grams_from_moles(mol: f64, molar_mass_g_per_mol: f64) -> f64 {
    mol * molar_mass_g_per_mol
}

pub fn element_counts(components: &[FormulaComponent], out: &mut Vec<(Element, u32)>) {
    for c in components {
        match c {
            FormulaComponent::Atom(el, n) => add_count(out, *el, *n),
            FormulaComponent::Group(inner, n) => {
                let mut sub = Vec::new();
                element_counts(inner, &mut sub);
                for (el, k) in sub {
                    add_count(out, el, k * *n);
                }
            }
        }
    }
}

fn add_count(out: &mut Vec<(Element, u32)>, el: Element, n: u32) {
    for (e, c) in out.iter_mut() {
        if *e == el {
            *c += n;
            return;
        }
    }
    out.push((el, n));
}
