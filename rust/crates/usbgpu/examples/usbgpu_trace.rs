use openpilot_usbgpu::{
    hardware::{self, RuntimeStatus},
    Error,
};
use serde::Deserialize;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Devices { path: PathBuf },
    Status { path: PathBuf, state: RuntimeStatus },
    Badge { state: RuntimeStatus },
    Power { bytes: Option<Vec<u8>> },
}
fn main() -> Result<(), Error> {
    for line in io::stdin().lock().lines() {
        let output = match serde_json::from_str::<Request>(&line?)? {
            Request::Devices { path } => serde_json::to_value(hardware::devices(&path)?)?,
            Request::Status { path, state } => {
                serde_json::json!(hardware::status(&hardware::devices(&path)?, state))
            }
            Request::Badge { state } => serde_json::json!(hardware::badge(state)),
            Request::Power { bytes } => {
                let power = bytes
                    .as_deref()
                    .map(hardware::PowerStatus::decode)
                    .transpose();
                match power {
                    Ok(power) => {
                        serde_json::json!({"value":power,"error":hardware::power_diagnostic(power)})
                    }
                    Err(error) => {
                        serde_json::json!({"error":format!("power status unavailable: {error}")})
                    }
                }
            }
        };
        println!("{output}");
        io::stdout().flush()?;
    }
    Ok(())
}
