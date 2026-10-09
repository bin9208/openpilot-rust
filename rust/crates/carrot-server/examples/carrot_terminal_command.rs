use openpilot_carrot_server::{terminal_commands, vision_test};
use std::path::PathBuf;
#[path = "carrot_terminal/config.rs"]
mod config;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--config") {
        return Err("owned CLI needs --config".into());
    }
    let path = PathBuf::from(args.next().ok_or("owned CLI config missing")?);
    let args: Vec<_> = args.collect();
    let (commands, vision) = config::configuration(&path)?;
    let code = if args.first().is_some_and(|arg| arg == "--vision-run") {
        vision_test::run(&vision).await?
    } else {
        terminal_commands::run(&commands, &args).await?
    };
    std::process::exit(code);
}
