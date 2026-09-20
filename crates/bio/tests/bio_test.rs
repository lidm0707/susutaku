#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use bio::{AminoAcid, Base, Dna};

#[test]
fn parse_and_gc() {
    let d = Dna::parse("GATTACA").unwrap();
    assert_eq!(d.len(), 7);
    assert_eq!(d.bases()[0], Base::G);
    assert!((d.gc_content() - 2.0 / 7.0).abs() < 1e-9);
    assert_eq!(Dna::parse("XYZ"), None);
    assert_eq!(Dna::parse("").unwrap().gc_content(), 0.0);
}

#[test]
fn complement_reverse_order() {
    let d = Dna::parse("ATGC").unwrap();
    let c = d.complement();
    let got: String = c.bases().iter().map(|b| b.to_string()).collect();
    assert_eq!(got, "GCAT");
}

#[test]
fn transcription_swaps_t_for_u() {
    let d = Dna::parse("ATGC").unwrap();
    let rna = d.transcribe();
    let s: String = rna.iter().map(|b| b.to_string()).collect();
    assert_eq!(s, "AUGC");
}

#[test]
fn translate_start_codon() {
    let d = Dna::parse("ATGGGCACT").unwrap(); // AUG GGC ACU
    let aa = d.translate();
    assert_eq!(aa, vec![AminoAcid::Met, AminoAcid::Gly, AminoAcid::Thr]);
}

#[test]
fn translate_stops_at_stop_codon() {
    let d = Dna::parse("TATTAGGGC").unwrap(); // UAU UAG GGC
    assert_eq!(d.translate(), vec![AminoAcid::Tyr]);
}

#[test]
fn codon_table_spot_checks() {
    assert_eq!(AminoAcid::Trp.code(), 'W');
    assert_eq!(AminoAcid::Stop.code(), '*');
}
