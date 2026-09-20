use zai_api::client::ZaiClient;

/// A proxy url set via `with_proxy` must be kept, empty strings dropped.
#[test]
fn with_proxy_stores_url_and_ignores_empty() {
    let client = ZaiClient::from_key("k", "glm-4.6").with_proxy("http://127.0.0.1:7890");
    assert_eq!(client.proxy_url(), Some("http://127.0.0.1:7890"));

    let client = ZaiClient::from_key("k", "glm-4.6").with_proxy("   ");
    assert_eq!(client.proxy_url(), None);

    let client = ZaiClient::from_key("k", "glm-4.6");
    assert_eq!(client.proxy_url(), None);
}

/// `with_proxy_opt(None)` leaves the client untouched (settings shorthand).
#[test]
fn with_proxy_opt_none_is_a_no_op() {
    let client = ZaiClient::from_key("k", "glm-4.6").with_proxy_opt(None);
    assert_eq!(client.proxy_url(), None);

    let client = ZaiClient::from_key("k", "glm-4.6").with_proxy_opt(Some("http://p:1"));
    assert_eq!(client.proxy_url(), Some("http://p:1"));
}
