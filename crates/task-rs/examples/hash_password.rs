//! Print an argon2 hash for a password — seeds the e2e admin for the
//! Playwright suite (see docs/playwright-workflow.md).

fn main() {
    let password = std::env::args().nth(1).expect("password argument required");
    println!(
        "{}",
        task_rs::user::hash_password(&password).expect("hash")
    );
}
