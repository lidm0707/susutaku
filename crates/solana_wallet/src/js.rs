//! JS bindings: what the browser actually imports.

use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use crate::keypair::WalletKeypair;
use crate::rpc::{SolanaRpc, DEVNET_URL};
use crate::store::WalletStore;

#[wasm_bindgen]
pub struct SusuWallet {
    kp: WalletKeypair,
    rpc: SolanaRpc,
}

#[wasm_bindgen]
impl SusuWallet {
    /// Load from localStorage, or generate + persist a new wallet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            kp: WalletStore::load_or_create(),
            rpc: SolanaRpc::new(DEVNET_URL),
        }
    }

    #[wasm_bindgen(getter, js_name = address)]
    pub fn address(&self) -> String {
        self.kp.address()
    }

    /// Balance in lamports.
    pub async fn balance(&self) -> Result<u64, JsValue> {
        self.rpc.get_balance(&self.kp.address()).await.map_err(|e| e.into())
    }

    /// Balance in SOL (float).
    pub async fn balance_sol(&self) -> Result<f64, JsValue> {
        self.rpc.get_balance_sol(&self.kp.address()).await.map_err(|e| e.into())
    }

    /// Devnet airdrop, lamports (use LAMPORTS_PER_SOL from exports).
    pub async fn airdrop(&self, lamports: u64) -> Result<String, JsValue> {
        self.rpc.request_airdrop(&self.kp, lamports).await.map_err(|e| e.into())
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.kp.sign(message)
    }

    /// Verify a signature against a base58 address.
    pub fn verify(address: &str, message: &[u8], signature: &[u8]) -> bool {
        WalletKeypair::verify(address, message, signature)
    }

    /// Arbitrary RPC call; params as any JSON-serializable value.
    pub async fn rpc_call(&self, method: &str, params: JsValue) -> Result<JsValue, JsValue> {
        let params: serde_json::Value = serde_wasm_bindgen::from_value(params)?;
        let v = self
            .rpc
            .call(method, params)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        to_value(&v).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Wipe the stored secret.
    pub fn forget() {
        WalletStore::clear();
    }
}

impl Default for SusuWallet {
    fn default() -> Self {
        Self::new()
    }
}
