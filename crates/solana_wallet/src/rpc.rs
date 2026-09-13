use crate::keypair::{WalletKeypair, LAMPORTS_PER_SOL};
use gloo::net::http::Request;
use serde_json::{json, Value};

pub const DEVNET_URL: &str = "https://api.devnet.solana.com";

/// Async JSON-RPC client over `gloo::net::http::Request`.
pub struct SolanaRpc {
    url: String,
}

impl SolanaRpc {
    pub fn new(url: &str) -> Self {
        Self { url: url.to_string() }
    }

    pub async fn get_balance(&self, address: &str) -> Result<u64, String> {
        let v = self
            .call("getBalance", json!([address]))
            .await?;
        v["result"]["value"]
            .as_u64()
            .ok_or_else(|| format!("unexpected balance response: {v}"))
    }

    pub async fn get_balance_sol(&self, address: &str) -> Result<f64, String> {
        Ok(self.get_balance(address).await? as f64 / LAMPORTS_PER_SOL as f64)
    }

    pub async fn request_airdrop(&self, keypair: &WalletKeypair, lamports: u64) -> Result<String, String> {
        let v = self
            .call(
                "requestAirdrop",
                json!([keypair.address(), lamports]),
            )
            .await?;
        v["result"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("airdrop failed: {v}"))
    }

    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp = Request::post(&self.url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.ok() {
            return Err(format!("rpc http status {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

pub fn devnet() -> SolanaRpc {
    SolanaRpc::new(DEVNET_URL)
}
