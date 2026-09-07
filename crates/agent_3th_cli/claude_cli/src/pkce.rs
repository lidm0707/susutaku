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
