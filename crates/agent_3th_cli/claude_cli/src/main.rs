use ai_interface_layer::run_prompt;
use zai_api::client::ZaiClient;

pub const CLI_NAME: &str = "claude-cli";

fn main() {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if prompt.is_empty() {
        eprintln!("usage: {CLI_NAME} <prompt>");
        std::process::exit(2);
    }
    match ZaiClient::from_env().map_err(|e| e.to_string()).and_then(|client| run_prompt(&client, &prompt).map_err(|e| e.to_string())) {
        Ok(content) => println!("{content}"),
        Err(e) => {
            eprintln!("{CLI_NAME}: {e}");
            std::process::exit(1);
        }
    }
}
