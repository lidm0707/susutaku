use backend::infra::client::install::{
    HUB_ADDR_ENV, REPO_URL, SERVER_URL_ENV, render_install_script,
};

const SERVER: &str = "http://10.0.0.5:3334";

#[test]
fn script_prepares_and_registers_runtime() {
    let s = render_install_script(SERVER);
    assert!(s.contains(REPO_URL));
    assert!(s.contains("cargo build --release -p backend"));
    assert!(s.contains(&format!(
        "export {SERVER_URL_ENV}=\"http://$server_host:8992\""
    )));
    assert!(s.contains(&format!("export {HUB_ADDR_ENV}=\"$server_host:8993\"")));
    assert!(s.contains("./target/release/backend"));
}
