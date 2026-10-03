mod firmware_wire;
use firmware_wire::{Input, Io};
use openpilot_can::Frame;
use openpilot_card::{
    firmware::Catalog,
    firmware_query::StartupIo,
    identification::{CachedParams, IdentifyOptions},
    isotp::Error,
    query::QueryIo,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    fixed: String,
    selected: Option<String>,
    skip: bool,
    disable_cache: bool,
    pandas: usize,
    cache: Option<CachedParams>,
    passive: Vec<Frame>,
    io: Input,
}
#[derive(Serialize)]
struct Transport {
    wire: Io,
    #[serde(skip)]
    passive: Vec<Frame>,
    logs: Vec<serde_json::Value>,
}
impl QueryIo for Transport {
    fn log(&mut self, level: openpilot_card::query::DiagnosticLevel, message: &str) {
        if matches!(
            message,
            "Using cached CarParams" | "Getting VIN & FW versions" | "Skipping VIN & FW query"
        ) || message.starts_with("VIN ")
        {
            self.logs.push(serde_json::json!([level, message]));
        }
    }
    fn receive(&mut self, wait_for_one: bool) -> Result<Vec<Vec<Frame>>, Error> {
        let mut packets = self.wire.receive(wait_for_one)?;
        packets.extend(vec![self.passive.clone(); 202]);
        Ok(packets)
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), Error> {
        self.wire.send(frames)
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.wire.sleep(seconds)
    }
    fn now(&mut self) -> f64 {
        self.wire.now()
    }
}
impl StartupIo for Transport {
    fn identification_event(&mut self, event: openpilot_card::identification::Event<'_>) {
        self.logs.push(serde_json::json!(["error", event]));
    }
    fn set_obd_multiplexing(&mut self, enabled: bool) -> Result<(), Error> {
        self.wire.set_obd_multiplexing(enabled)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let catalog = Catalog::load()?;
    let mut results = Vec::new();
    for case in cases {
        let mut transport = Transport {
            wire: Io::from(case.io),
            passive: case.passive,
            logs: Vec::new(),
        };
        let result = catalog.identify(
            IdentifyOptions {
                fixed_fingerprint: &case.fixed,
                selected_car: case.selected.as_deref(),
                skip_fw_query: case.skip,
                disable_fw_cache: case.disable_cache,
                pandas: case.pandas,
            },
            case.cache.as_ref(),
            &mut transport,
        )?;
        results
            .push(serde_json::json!({"result":result,"io":transport.wire,"logs":transport.logs}));
    }
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&results)?)?;
    Ok(())
}
