use openpilot_carrot_server::vision_test::{self, Config};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() != ["_run"] {
        eprintln!("usage: openpilot-vision-test _run");
        std::process::exit(2);
    }
    let result = async { vision_test::run(&Config::original()?).await }.await;
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[vision_test] {error}");
            1
        }
    };
    std::process::exit(code);
}
