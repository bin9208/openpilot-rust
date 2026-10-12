use openpilot_carrot_server::youtube_test::{self, Config};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if youtube_test::help(&args) {
        return;
    }
    let result = async { youtube_test::run_command(&Config::original()?, &args).await }.await;
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[youtube-test] {error}");
            1
        }
    };
    std::process::exit(code);
}
