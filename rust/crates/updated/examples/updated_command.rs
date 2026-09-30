use openpilot_logging::producer::Factory;
use openpilot_updated::{
    process::{Commands, NativeCommands},
    signals::{Signals, Wake},
    Error,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
};
#[derive(Deserialize)]
struct Config {
    launcher: PathBuf,
    argv: Vec<String>,
    cwd: Option<PathBuf>,
    endpoint: String,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let config: Config = serde_json::from_str(&text)?;
    let wake = Arc::new(Wake::default());
    let _signals = Signals::install(Arc::clone(&wake), Factory::new(config.endpoint)?)?;
    let mut runner = NativeCommands {
        launcher: config.launcher,
        wake,
    };
    let value = match runner.run(&config.argv, config.cwd.as_deref()) {
        Ok(output) => json!({"output":output,"code":0}),
        Err(Error::Command { output, code, .. }) => json!({"output":output,"code":code}),
        Err(Error::Interrupted) => json!({"interrupted":true}),
        Err(error) => json!({"error":error.to_string()}),
    };
    println!("{value}");
    Ok(())
}
