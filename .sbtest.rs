fn main() {
    std::env::set_current_dir("/app").unwrap();
    let out = core_agent::sandbox_jail::run("whoami");
    match out { Ok(o) => println!("OUT: {o}"), Err(e) => println!("ERR: {e}") }
}
