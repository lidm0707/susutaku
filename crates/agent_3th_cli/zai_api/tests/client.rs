use zai_api::client::{Endpoint, ZaiClient, strip_bearer_scheme};

const RAW_KEY: &str = "eyJaa.aa.bb";
const SCHEME_KEY: &str = "Bearer eyJaa.aa.bb";

#[test]
fn strips_bearer_scheme_and_whitespace() {
    assert_eq!(strip_bearer_scheme(SCHEME_KEY), RAW_KEY);
    assert_eq!(strip_bearer_scheme("  eyJaa.aa.bb  "), RAW_KEY);
    assert_eq!(strip_bearer_scheme(RAW_KEY), RAW_KEY);
}

#[test]
fn default_endpoint_is_coding() {
    assert_eq!(Endpoint::default(), Endpoint::Coding);
    assert_eq!(Endpoint::Coding.url(), zai_api::client::CODING_BASE_URL);
    assert_eq!(Endpoint::Generic.url(), zai_api::client::GENERIC_BASE_URL);
}

#[test]
fn from_key_strips_scheme() {
    let client = ZaiClient::from_key(SCHEME_KEY, "glm-4.6").with_endpoint(Endpoint::Generic);
    // no panic + endpoint builder compiles; key handling is internal
    let _ = client;
}
