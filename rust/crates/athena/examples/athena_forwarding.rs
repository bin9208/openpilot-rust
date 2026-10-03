use openpilot_athena::{
    forwarding, runtime,
    state::{Config, Shared, Stop},
    Error,
};
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};
fn main() -> Result<(), Error> {
    match std::env::args().nth(1).as_deref() {
        Some("scan") => {
            let mut attrs = forwarding::Attributes::default();
            for line in io::stdin().lock().lines() {
                let row: serde_json::Value = serde_json::from_str(&line?)?;
                println!(
                    "{}",
                    serde_json::to_string(&forwarding::logs(
                        std::path::Path::new(row["root"].as_str().ok_or(Error::Contract("root"))?),
                        row["now"].as_i64().ok_or(Error::Contract("time"))?,
                        &mut attrs
                    )?)?
                );
                io::stdout().flush()?;
            }
        }
        Some("session") => {
            let mut config = Config::for_runtime()?;
            config.pc = false;
            let shared = Arc::new(Shared::new(config)?);
            let stop = Stop::default();
            signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.signal_flag())?;
            runtime::run(shared, &stop)?;
        }
        _ => return Err(Error::Contract("expected scan or session")),
    }
    Ok(())
}
