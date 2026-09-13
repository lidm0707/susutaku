use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const SECRET_KEY_LEN: usize = 32;

/// ed25519 wallet keypair; address is base58 of the public key (32 bytes),
/// same encoding as Solana pubkeys.
pub struct WalletKeypair {
    signing: SigningKey,
}

impl WalletKeypair {
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    pub fn from_secret_bytes(bytes: &[u8]) -> anyhow_std::Result<Self> {
        let arr: [u8; SECRET_KEY_LEN] = bytes
            .try_into()
            .map_err(|_| anyhow_std::Error::msg("secret seed must be 32 bytes"))?;
        Ok(Self {
            signing: SigningKey::from_bytes(&arr),
        })
    }

    pub fn secret_bytes(&self) -> [u8; SECRET_KEY_LEN] {
        self.signing.to_bytes()
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    pub fn address(&self) -> String {
        bs58::encode(self.public_bytes()).into_string()
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.signing.sign(message).to_bytes().to_vec()
    }

    pub fn verify(address: &str, message: &[u8], signature: &[u8]) -> bool {
        let Ok(pub_bytes) = bs58::decode(address).into_vec() else {
            return false;
        };
        let (Some(pk), Some(sig)) = (
            pub_bytes
                .try_into()
                .ok()
                .and_then(|b: [u8; 32]| VerifyingKey::from_bytes(&b).ok()),
            <[u8; 64]>::try_from(signature).ok(),
        ) else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig);
        pk.verify(message, &sig).is_ok()
    }
}

/// Minimal local error shim so the crate stays dependency-lean on wasm.
pub mod anyhow_std {
    use std::fmt;

    #[derive(Debug)]
    pub struct Error(String);

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl Error {
        pub fn msg(s: &str) -> Self {
            Self(s.to_string())
        }
    }

    pub type Result<T> = std::result::Result<T, Error>;
}
