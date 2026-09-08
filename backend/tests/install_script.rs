use backend::infra::install::{
    HUB_ADDR_ENV, REPO_URL, Role, SERVER_URL_ENV, render_install_script,
};

const SERVER: &str = "http://10.0.0.5:8992";

#[test]
fn script_prepares_and_registers_worker() {
    let s = render_install_script(Role::Worker, SERVER);
    assert!(s.contains(REPO_URL));
    assert!(s.contains("cargo build --release -p model-server -p backend"));
    assert!(s.contains(&format!(
        "export {SERVER_URL_ENV}=\"http://$server_host:8992\""
    )));
    assert!(s.contains(&format!("export {HUB_ADDR_ENV}=\"$server_host:8993\"")));
    assert!(s.contains("./target/release/backend"));
}

#[test]
fn script_model_role_runs_model_server() {
    let s = render_install_script(Role::Model, SERVER);
    assert!(s.contains("./target/release/model-server"));
    assert!(s.contains("role: $role ("));
}

#[test]
fn script_auto_role_decides_from_probe() {
    let s = render_install_script(Role::Auto, SERVER);
    assert!(s.contains("if [ \"$capable\" = yes ]; then role=model; else role=worker; fi"));
}
