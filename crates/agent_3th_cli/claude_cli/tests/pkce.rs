use claude_cli::pkce::{challenge, new_verifier};

#[test]
fn challenge_matches_rfc7636_vector() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        challenge(verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn verifier_is_url_safe() {
    let v = new_verifier();
    assert_eq!(v.len(), 43);
    assert!(!v.contains('+') && !v.contains('/') && !v.contains('='));
}
