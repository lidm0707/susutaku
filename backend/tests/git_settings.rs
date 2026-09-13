//! Git repo settings: url validation, credential redaction.

use backend::infra::settings::git;

#[test]
fn rejects_non_http_schemes() {
    for url in [
        "",
        "file:///etc/passwd",
        "ftp://example.com/repo.git",
        "git@github.com:org/repo.git",
        "example.com/repo.git",
    ] {
        assert!(git::validate_url(url).is_err(), "must reject {url:?}");
    }
}

#[test]
fn accepts_plain_https_urls() {
    for url in [
        "https://github.com/org/repo.git",
        "http://gitea.local:3000/org/repo.git",
    ] {
        assert!(git::validate_url(url).is_ok(), "must accept {url:?}");
    }
}

#[test]
fn rejects_embedded_credentials() {
    for url in [
        "https://user:ghp_token@github.com/org/repo.git",
        "https://x-access-token:abc@github.com/org/repo.git",
        "http://token@example.com/repo.git",
    ] {
        assert!(git::validate_url(url).is_err(), "must reject {url:?}");
    }
    assert!(git::validate_url("https://github.com/user/repo.git").is_ok());
}

#[test]
fn redacts_userinfo_but_keeps_plain_url() {
    assert_eq!(
        git::redact_url("https://user:secret@github.com/org/repo.git"),
        "https://github.com/org/repo.git"
    );
    assert_eq!(
        git::redact_url("https://github.com/org/repo.git"),
        "https://github.com/org/repo.git"
    );
}

#[test]
fn secret_validation_rejects_whitespace_and_overlong() {
    assert!(git::validate_secret("ghp_abc").is_ok());
    assert!(git::validate_secret("has space").is_err());
    assert!(git::validate_secret(&"x".repeat(513)).is_err());
}
