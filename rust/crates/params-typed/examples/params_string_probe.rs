use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_params::{Params, KEYS};
use openpilot_params_typed::{get_string, Error};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Command {
    key: String,
    marker: String,
    #[serde(default)]
    close: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let params = Params::open(std::path::Path::new(&args[1]), &args[2])?;
    let mut logger = Factory::new(args[3].clone())?.logger();
    println!(
        "{}",
        json!({"keys": KEYS.iter().filter(|key| key.kind == 0).map(|key| key.name).collect::<Vec<_>>()})
    );
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let command: Command = serde_json::from_str(&line?)?;
        if command.close {
            logger.close();
        }
        let result = match get_string(&params, &command.key, &mut logger) {
            Ok(value) => json!({"value": value}),
            Err(Error::Params(openpilot_params::Error::UnknownKey(_))) => {
                json!({"error": "UnknownKey"})
            }
            Err(Error::Logging(_)) => json!({"error": "Logging"}),
            Err(error) => return Err(error.into()),
        };
        if !command.close {
            logger.emit(log_site!(), Record::text(Level::Debug, command.marker))?;
        }
        println!("{result}");
        io::stdout().flush()?;
    }
    Ok(())
}
