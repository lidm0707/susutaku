//! Integration test: user auth (argon2) + roles against a live Postgres.

use kanban_rs::{NewUser, Role, Store};

const PASSWORD_MIN_LEN: usize = 8;

fn unique_name(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{tag}_{nanos}")
}

#[tokio::test]
async fn user_auth_roundtrip() {
    let store = Store::connect(Store::default_url()).await.expect("connect");
    let username = unique_name("auth_user");
    let password = "s3cret-password";

    // Too-short password is rejected before hashing.
    let short = store
        .create_user(&NewUser {
            username: &username,
            password: "short",
            role: Role::Viewer,
        })
        .await;
    assert!(matches!(short, Err(kanban_rs::StoreError::PasswordTooShort)));
    assert_eq!(PASSWORD_MIN_LEN, 8);

    let user = store
        .create_user(&NewUser {
            username: &username,
            password,
            role: Role::Editor,
        })
        .await
        .expect("create user");
    assert_eq!(user.role, Role::Editor.as_str());

    // Duplicate username is rejected.
    assert!(store
        .create_user(&NewUser {
            username: &username,
            password,
            role: Role::Viewer,
        })
        .await
        .is_err());

    // Wrong password does not open a session.
    assert!(store
        .login(&username, "wrong-password")
        .await
        .expect("login")
        .is_none());

    // Correct password opens a session that resolves to the user.
    let token = store
        .login(&username, password)
        .await
        .expect("login")
        .expect("token");
    let authed = store.auth(&token).await.expect("auth").expect("user");
    assert_eq!(authed.username, username);
    assert_eq!(authed.role, Role::Editor.as_str());

    // Role ranking: editor can edit but not manage users.
    let role = Role::Editor;
    assert!(role.can_edit());
    assert!(!role.can_manage_users());
    assert!(Role::Admin.can_manage_users());
    assert!(Role::Owner > Role::SuperAdmin);
    assert!(Role::SuperAdmin > Role::Admin);
    assert!(Role::Admin > Role::Editor);
    assert!(Role::Editor > Role::Viewer);

    store.logout(&token).await.expect("logout");
    assert!(store.auth(&token).await.expect("auth").is_none());

    let users = store.list_users().await.expect("list");
    assert!(users.iter().any(|u| u.username == username));
}

#[tokio::test]
async fn default_admin_seeded_and_forced_password_change() {
    let store = Store::connect(Store::default_url()).await.expect("connect");
    store.ensure_default_admin().await.expect("seed");

    // Default owner/owner login opens a session flagged for password change.
    let token = store
        .login(kanban_rs::DEFAULT_ADMIN_USER, kanban_rs::DEFAULT_ADMIN_PASSWORD)
        .await
        .expect("login")
        .expect("token");
    let user = store.auth(&token).await.expect("auth").expect("user");
    assert_eq!(user.role, Role::Owner.as_str());
    assert!(user.must_change_password);

    // Wrong old password is rejected.
    let bad = store
        .change_password(user.id, "wrong-old", "new-password-123")
        .await;
    assert!(matches!(bad, Err(kanban_rs::StoreError::BadCredentials)));

    // Too-short new password is rejected.
    let short = store
        .change_password(user.id, kanban_rs::DEFAULT_ADMIN_PASSWORD, "short")
        .await;
    assert!(matches!(short, Err(kanban_rs::StoreError::PasswordTooShort)));

    store.logout(&token).await.expect("logout");
}
