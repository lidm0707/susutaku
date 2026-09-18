//! Users, argon2 password hashing and ranked roles.

use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use serde::{Deserialize, Serialize};

use crate::store::StoreError;

pub const ROLE_OWNER: &str = "owner";
pub const ROLE_SUPER_ADMIN: &str = "super_admin";
pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_EDITOR: &str = "editor";
pub const ROLE_VIEWER: &str = "viewer";

pub const MIN_PASSWORD_LEN: usize = 8;

pub const DEFAULT_ADMIN_USER: &str = "owner";
pub const DEFAULT_ADMIN_PASSWORD: &str = "owner";

/// Ranked roles: derived `Ord` follows variant order (Viewer < … < Owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    Viewer,
    Editor,
    Admin,
    SuperAdmin,
    Owner,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => ROLE_VIEWER,
            Role::Editor => ROLE_EDITOR,
            Role::Admin => ROLE_ADMIN,
            Role::SuperAdmin => ROLE_SUPER_ADMIN,
            Role::Owner => ROLE_OWNER,
        }
    }

    pub fn can_edit(self) -> bool {
        self >= Role::Editor
    }

    pub fn can_manage_users(self) -> bool {
        self >= Role::Admin
    }
}

impl core::str::FromStr for Role {
    type Err = StoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            ROLE_VIEWER => Ok(Role::Viewer),
            ROLE_EDITOR => Ok(Role::Editor),
            ROLE_ADMIN => Ok(Role::Admin),
            ROLE_SUPER_ADMIN => Ok(Role::SuperAdmin),
            ROLE_OWNER => Ok(Role::Owner),
            other => Err(StoreError::BadRole(other.to_string())),
        }
    }
}

/// Public user view — never carries the password hash.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
}

pub struct NewUser<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub role: Role,
}

pub fn hash_password(password: &str) -> Result<String, StoreError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| StoreError::Hash(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}
