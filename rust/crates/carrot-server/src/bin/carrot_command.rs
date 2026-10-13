use openpilot_carrot_server::terminal_commands::{self, Config};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = async { terminal_commands::run(&Config::original()?, &args).await }.await;
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[terminal] {error}");
            1
        }
    };
    std::process::exit(code);
}
