use openpilot_logging::producer::Factory;
use openpilot_navd::native::{config, Options};
use openpilot_params::Params;
use serde_json::json;
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let root: String = serde_json::from_str(&line?)?;
        let options = Options {
            persist_root: Some(root.into()),
            ..Options::default()
        };
        let params = Params::for_runtime()?;
        let factory = Factory::for_runtime()?;
        let result = match config::load(&params, &mut factory.logger(), &options) {
            Ok(config) => json!({"ok": true, "host": config.host, "token": config.token}),
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        println!("{result}");
    }
    Ok(())
}
