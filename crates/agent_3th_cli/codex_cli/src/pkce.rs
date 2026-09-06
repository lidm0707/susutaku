use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

const VERIFIER_BYTES: usize = 32;

pub fn new_verifier() -> String {
    let mut bytes = [0u8; VERIFIER_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
