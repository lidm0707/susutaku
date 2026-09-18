//! Postgres adapter for the `users` + `auth_sessions` tables.

use task_rs::user::{hash_password, verify_password};
use task_rs::{
    DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER, MIN_PASSWORD_LEN, NewUser, Role, StoreError,
    UserRow,
};

use super::Store;

const SESSION_BYTES: usize = 32;

fn new_token() -> Result<String, StoreError> {
    use argon2::password_hash::rand_core::RngCore;
    let mut bytes = [0u8; SESSION_BYTES];
    argon2::password_hash::rand_core::OsRng.fill_bytes(&mut bytes);
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug)]
struct UserAuth {
    id: i64,
    password_hash: String,
}

impl Store {
    pub async fn user_count(&self) -> Result<i64, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "count!" FROM users"#)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.count)
    }

    pub async fn create_user(&self, user: &NewUser<'_>) -> Result<UserRow, StoreError> {
        if user.password.len() < MIN_PASSWORD_LEN {
            return Err(StoreError::PasswordTooShort);
        }
        let hash = hash_password(user.password)?;
        let row = sqlx::query_as!(
            UserRow,
            r#"INSERT INTO users (username, password_hash, role)
               VALUES ($1, $2, $3)
               RETURNING id, username, role, must_change_password"#,
            user.username,
            hash,
            user.role.as_str(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some("23505") => StoreError::UsernameTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(row)
    }

    pub async fn list_users(&self) -> Result<Vec<UserRow>, StoreError> {
        let rows = sqlx::query_as!(
            UserRow,
            r#"SELECT id, username, role, must_change_password FROM users ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Verify credentials and open a session; returns the bearer token.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<String>, StoreError> {
        let row = sqlx::query_as!(
            UserAuth,
            r#"SELECT id, password_hash FROM users WHERE username = $1"#,
            username
        )
        .fetch_optional(&self.pool)
        .await?;
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        if !verify_password(password, &row.password_hash) {
            return Ok(None);
        }
        let token = new_token()?;
        sqlx::query!(
            r#"INSERT INTO auth_sessions (token, user_id) VALUES ($1, $2)"#,
            token,
            row.id
        )
        .execute(&self.pool)
        .await?;
        Ok(Some(token))
    }

    /// Seed the default owner/owner account; no-op if the username exists.
    pub async fn ensure_default_admin(&self) -> Result<(), StoreError> {
        let hash = hash_password(DEFAULT_ADMIN_PASSWORD)?;
        sqlx::query!(
            r#"INSERT INTO users (username, password_hash, role, must_change_password)
               VALUES ($1, $2, $3, TRUE)
               ON CONFLICT (username) DO NOTHING"#,
            DEFAULT_ADMIN_USER,
            hash,
            Role::Owner.as_str(),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Change the session user's password after verifying the old one;
    /// clears the must-change flag.
    pub async fn change_password(
        &self,
        user_id: i64,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), StoreError> {
        if new_password.len() < MIN_PASSWORD_LEN {
            return Err(StoreError::PasswordTooShort);
        }
        let row = sqlx::query!(r#"SELECT password_hash FROM users WHERE id = $1"#, user_id)
            .fetch_one(&self.pool)
            .await?;
        if !verify_password(old_password, &row.password_hash) {
            return Err(StoreError::BadCredentials);
        }
        let hash = hash_password(new_password)?;
        sqlx::query!(
            r#"UPDATE users SET password_hash = $2, must_change_password = FALSE WHERE id = $1"#,
            user_id,
            hash,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn auth(&self, token: &str) -> Result<Option<UserRow>, StoreError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"SELECT u.id, u.username, u.role, u.must_change_password
               FROM auth_sessions s JOIN users u ON u.id = s.user_id
               WHERE s.token = $1"#,
            token
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn logout(&self, token: &str) -> Result<(), StoreError> {
        sqlx::query!(r#"DELETE FROM auth_sessions WHERE token = $1"#, token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
