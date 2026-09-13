use chemi::{
    element_counts, grams_from_moles, molar_mass_of, moles_from_grams, parse_formula, Element,
    FormulaComponent,
};

const EPS: f64 = 1e-6;

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "{a} != {b}");
}

#[test]
fn parse_simple() {
    let f = parse_formula("H2O").unwrap();
    assert_eq!(
        f,
        vec![
            FormulaComponent::Atom(Element::H, 2),
            FormulaComponent::Atom(Element::O, 1)
        ]
    );
}

#[test]
fn parse_group() {
    let f = parse_formula("Ca(OH)2").unwrap();
    assert_eq!(f[0], FormulaComponent::Atom(Element::Ca, 1));
    match &f[1] {
        FormulaComponent::Group(inner, 2) => assert_eq!(
            inner,
            &vec![
                FormulaComponent::Atom(Element::O, 1),
                FormulaComponent::Atom(Element::H, 1)
            ]
        ),
        other => panic!("unexpected: {other:?}"),
    }
    assert!(parse_formula("Ca(OH").is_none());
    assert!(parse_formula("Xx2").is_none());
    assert!(parse_formula("H2O3x").is_none());
}

#[test]
fn molar_mass_water() {
    approx(molar_mass_of("H2O").unwrap(), 2.016 + 15.999);
}

#[test]
fn molar_mass_copper_sulfate_group() {
    let m = molar_mass_of("CuSO4").unwrap();
    approx(m, 63.546 + 32.06 + 4.0 * 15.999);
}

#[test]
fn nested_groups() {
    let m = molar_mass_of("Ca3(PO4)2").unwrap();
    let expected = 3.0 * 40.078 + 2.0 * (30.974 + 4.0 * 15.999);
    approx(m, expected);
}

#[test]
fn element_counts_expand_groups() {
    let f = parse_formula("Ca3(PO4)2").unwrap();
    let mut counts = Vec::new();
    element_counts(&f, &mut counts);
    assert!(counts.contains(&(Element::Ca, 3)));
    assert!(counts.contains(&(Element::P, 2)));
    assert!(counts.contains(&(Element::O, 8)));
}

#[test]
fn mole_conversions() {
    let water = molar_mass_of("H2O").unwrap();
    approx(moles_from_grams(18.015, water), 1.0);
    approx(grams_from_moles(1.0, water), water);
}

#[test]
fn atomic_basics() {
    assert_eq!(Element::O.atomic_number(), 8);
    assert_eq!(Element::Au.symbol(), "Au");
    assert_eq!(Element::from_symbol("Au"), Some(Element::Au));
    assert_eq!(Element::from_symbol("Xx"), None);
}
