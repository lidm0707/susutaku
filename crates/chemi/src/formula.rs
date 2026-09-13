use crate::element::Element;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaComponent {
    Atom(Element, u32),
    Group(Vec<FormulaComponent>, u32),
}

pub fn parse_formula(s: &str) -> Option<Vec<FormulaComponent>> {
    let bytes: Vec<char> = s.chars().collect();
    let mut pos = 0;
    let (comps, rest) = parse_sequence(&bytes, &mut pos)?;
    if !rest.is_empty() {
        return None;
    }
    Some(comps)
}

fn parse_sequence<'a>(
    input: &'a [char],
    pos: &mut usize,
) -> Option<(Vec<FormulaComponent>, &'a [char])> {
    let mut comps = Vec::new();
    while *pos < input.len() {
        let c = input[*pos];
        if c == ')' {
            break;
        }
        if c.is_ascii_uppercase() {
            let sym = symbol_at(input, pos)?;
            let el = Element::from_symbol(&sym)?;
            let n = number_after(input, pos).unwrap_or(1);
            comps.push(FormulaComponent::Atom(el, n));
        } else if c == '(' {
            *pos += 1;
            let (inner, _) = parse_sequence(input, pos)?;
            if input.get(*pos) != Some(&')') {
                return None;
            }
            *pos += 1;
            let n = number_after(input, pos).unwrap_or(1);
            comps.push(FormulaComponent::Group(inner, n));
        } else {
            return None;
        }
    }
    Some((comps, &input[*pos..]))
}

fn symbol_at(input: &[char], pos: &mut usize) -> Option<String> {
    let mut sym = String::new();
    sym.push(input[*pos]);
    *pos += 1;
    if *pos < input.len() && input[*pos].is_ascii_lowercase() {
        sym.push(input[*pos]);
        *pos += 1;
    }
    Some(sym)
}

fn number_after(input: &[char], pos: &mut usize) -> Option<u32> {
    let start = *pos;
    while *pos < input.len() && input[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if start == *pos {
        return None;
    }
    let s: String = input[start..*pos].iter().collect();
    s.parse().ok()
}
