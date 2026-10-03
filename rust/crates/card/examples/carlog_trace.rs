use openpilot_card::{
    ecu::EcuAddress,
    firmware_query::StartupIo,
    identification::Event,
    query::{DiagnosticLevel, QueryIo},
    runtime::NativeIo,
};
use openpilot_params::Params;
use serde::Deserialize;
use std::{
    io::{self, Read},
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum InputLevel {
    Warning,
    Error,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Text { level: InputLevel, message: String },
    MalformedVin { vin: String },
    Unmatched { fingerprints: String },
    Fingerprinted { time: f64 },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = String::new();
    io::stdin().read_to_string(&mut bytes)?;
    let inputs: Vec<Input> = serde_json::from_str(&bytes)?;
    let mut io = NativeIo::new(Params::for_runtime()?, Arc::new(AtomicBool::new(false)))?;
    for input in inputs {
        match input {
            Input::Text { level, message } => io.log(
                match level {
                    InputLevel::Warning => DiagnosticLevel::Warning,
                    InputLevel::Error => DiagnosticLevel::Error,
                },
                &message,
            ),
            Input::MalformedVin { vin } => {
                io.identification_event(Event::MalformedVin { vin: &vin })
            }
            Input::Unmatched { fingerprints } => {
                io.identification_event(Event::Unmatched { fingerprints })
            }
            Input::Fingerprinted { time } => io.identification_event(Event::Fingerprinted {
                car_fingerprint: "MOCK",
                source: 2,
                fuzzy: false,
                cached: true,
                fw_count: 1,
                ecu_responses: &[EcuAddress(123, None, 0), EcuAddress(456, Some(8), 2)],
                vin_rx_addr: -1,
                vin_rx_bus: -1,
                fingerprints: "{0: {123: 8}}".into(),
                fw_query_time: time,
            }),
        }
    }
    Ok(())
}
