use gloo::storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

use crate::keypair::WalletKeypair;

const STORE_KEY: &str = "susutaku::solana_wallet";

#[derive(Serialize, Deserialize)]
struct StoredWallet {
    /// base58 of the 32-byte ed25519 seed
    secret: String,
}

impl StoredWallet {
    fn from_keypair(kp: &WalletKeypair) -> Self {
        Self {
            secret: bs58::encode(kp.secret_bytes()).into_string(),
        }
    }

    fn to_keypair(&self) -> crate::keypair::anyhow_std::Result<WalletKeypair> {
        let bytes = bs58::decode(&self.secret)
            .into_vec()
            .map_err(|_| crate::keypair::anyhow_std::Error::msg("bad stored secret"))?;
        WalletKeypair::from_secret_bytes(&bytes)
    }
}

/// Persist the wallet secret in localStorage (gloo). Real apps should prefer
/// an encrypted seed phrase; this stores the raw seed for simplicity.
pub struct WalletStore;

impl WalletStore {
    pub fn save(kp: &WalletKeypair) {
        LocalStorage::set(STORE_KEY, StoredWallet::from_keypair(kp)).ok();
    }

    pub fn load() -> Option<WalletKeypair> {
        let stored: StoredWallet = LocalStorage::get(STORE_KEY).ok()?;
        stored.to_keypair().ok()
    }

    /// Load or create-and-save on first use.
    pub fn load_or_create() -> WalletKeypair {
        match Self::load() {
            Some(kp) => kp,
            None => {
                let kp = WalletKeypair::generate();
                Self::save(&kp);
                kp
            }
        }
    }

    pub fn clear() {
        LocalStorage::delete(STORE_KEY);
    }
}
