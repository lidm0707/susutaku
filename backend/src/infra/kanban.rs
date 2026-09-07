//! Infrastructure: Postgres-backed kanban store adapter.

use kanban_rs::Store;

pub const DATABASE_URL_ENV: &str = "DATABASE_URL";

pub async fn connect() -> Store {
    let url = std::env::var(DATABASE_URL_ENV).unwrap_or_else(|_| Store::default_url().into());
    let store = Store::connect(&url).await.expect("kanban postgres connect");
    store.ensure_default_admin().await.expect("seed default admin");
    store
}
