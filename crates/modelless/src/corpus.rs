//! English verb corpus: 1000 verbs x 3 forms (base, past, past participle)
//! = 3000 word forms.

mod irregular;
mod regular;

use irregular::IRREGULAR;
use regular::REGULAR;

pub const VERB_COUNT: usize = 173 + 827;
pub const FORMS_PER_VERB: usize = 3;
pub const WORD_COUNT: usize = VERB_COUNT * FORMS_PER_VERB;

const REGULAR_COUNT: usize = VERB_COUNT - IRREGULAR.len();
const _: () = assert!(REGULAR.len() == REGULAR_COUNT);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Base,
    Past,
    PastParticiple,
}

pub const FORMS: [Form; FORMS_PER_VERB] = [Form::Base, Form::Past, Form::PastParticiple];

/// Suffix rule applied to a regular base to inflect past / past participle.
#[derive(Clone, Copy)]
enum SuffixRule {
    Ed,
    D,
    Ied,
    DoubledConsonantEd,
}

/// Bases whose last consonant doubles before -ed (CVC pattern). Sorted.
const DOUBLING: &[&str] = &[
    "chat", "chop", "cup", "dot", "fit", "grab", "grin", "gum", "hum", "jam", "jog", "kid", "log",
    "mop", "nap", "nod", "pat", "peg", "pin", "pit", "plod", "pop", "pub", "rag", "ram", "rap",
    "rig", "rim", "rip", "rob", "rot", "rub", "rug", "rum", "sap", "sip", "skip", "slim", "slip",
    "slog", "snap", "snip", "sob", "stab", "stem", "step", "stop", "stub", "stun", "tag", "tan",
    "tap", "tip", "top", "tug", "wed", "wig", "wrap", "zip",
];

fn suffix_rule(base: &str) -> SuffixRule {
    let Some(last) = base.chars().next_back() else {
        return SuffixRule::Ed;
    };
    if last == 'e' {
        return SuffixRule::D;
    }
    let consonant = |c: char| c.is_ascii_alphabetic() && !"aeiou".contains(c);
    let before = base.chars().rev().nth(1);
    if last == 'y' && before.is_some_and(consonant) {
        return SuffixRule::Ied;
    }
    if DOUBLING.binary_search(&base).is_ok() {
        return SuffixRule::DoubledConsonantEd;
    }
    SuffixRule::Ed
}

fn inflect(base: &str, rule: SuffixRule) -> String {
    match rule {
        SuffixRule::Ed => format!("{base}ed"),
        SuffixRule::D => format!("{base}d"),
        SuffixRule::Ied => format!("{}ied", &base[..base.len() - 1]),
        SuffixRule::DoubledConsonantEd => {
            format!("{base}{last}ed", last = base.chars().next_back().unwrap())
        }
    }
}

/// One verb with its three word forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verb {
    pub base: &'static str,
    pub past: &'static str,
    pub past_participle: &'static str,
}

impl Verb {
    pub fn regular(base: &'static str) -> Self {
        let rule = suffix_rule(base);
        Self {
            base,
            past: leak(inflect(base, rule)),
            past_participle: leak(inflect(base, rule)),
        }
    }

    pub fn form(&self, form: Form) -> &'static str {
        match form {
            Form::Base => self.base,
            Form::Past => self.past,
            Form::PastParticiple => self.past_participle,
        }
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// All 1000 verbs, irregular first.
pub fn verbs() -> impl Iterator<Item = Verb> {
    IRREGULAR
        .iter()
        .map(|&(base, past, pp)| Verb {
            base,
            past,
            past_participle: pp,
        })
        .chain(REGULAR.iter().copied().map(Verb::regular))
}

/// All 3000 word forms, in verb order.
pub fn words() -> impl Iterator<Item = &'static str> {
    verbs().flat_map(|v| FORMS.map(|f| v.form(f)))
}

/// The whole corpus as one text blob (forms separated by newlines).
pub fn text() -> String {
    let mut out = String::with_capacity(WORD_COUNT * 8);
    for w in words() {
        out.push_str(w);
        out.push('\n');
    }
    out
}
