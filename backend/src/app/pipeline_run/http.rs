//! Raw HTTP fetch used by pipeline `fetch` nodes (blocking, runs via spawn_blocking).

pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const FETCH_MAX_BYTES: usize = 1 << 20;
pub const HTTP_GET: &str = "GET";
pub const NETWORK_HINT: &str =
    "(backend has no outbound network access — check host/container network)";

pub fn http_fetch(method: &str, url: &str, body: &str) -> Result<String, String> {
    use std::io::Read;
    let timeout = std::time::Duration::from_secs(FETCH_TIMEOUT_SECS);
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let resp = if method == HTTP_GET {
        agent.get(url).call()
    } else {
        agent.request(method, url).send_string(body)
    };
    match resp {
        Ok(resp) => {
            // Cap the read in bytes before buffering the whole body.
            let mut reader = resp.into_reader().take(FETCH_MAX_BYTES as u64);
            let mut text = String::new();
            reader
                .read_to_string(&mut text)
                .map(|_| text)
                .map_err(|e| format!("fetch {url}: failed to read body: {e}"))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let detail = resp.into_string().unwrap_or_default();
            Err(format!(
                "fetch {method} {url}: http {code} {detail} {NETWORK_HINT}"
            ))
        }
        Err(ureq::Error::Transport(t)) => {
            Err(format!("fetch {method} {url} failed: {t} {NETWORK_HINT}"))
        }
    }
}
