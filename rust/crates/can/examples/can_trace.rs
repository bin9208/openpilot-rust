use openpilot_can::{dbc::Dbc, packer::Packer, parser::Parser, Packet};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
};

#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    path: PathBuf,
    bus: u8,
    now: u64,
    messages: Vec<(String, Option<f64>, bool)>,
    #[serde(default)]
    ignored_messages: Vec<String>,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Step {
    Define,
    Lazy {
        name: String,
        signal: String,
        now: u64,
    },
    Pack {
        address: u32,
        values: Vec<(String, f64)>,
        rx_counter: Option<i64>,
    },
    Update {
        packets: Vec<Packet>,
        checks: usize,
    },
    Ready {
        enabled: bool,
    },
    Add {
        name: String,
        frequency: Option<f64>,
        ignore_counter: bool,
        now: u64,
    },
}
#[derive(Serialize)]
struct Snapshot {
    updated: Vec<u32>,
    checks: Vec<bool>,
    bus_timeout: bool,
    values: BTreeMap<u32, Vec<f64>>,
    all_values: BTreeMap<u32, Vec<Vec<f64>>>,
    counters: BTreeMap<u32, String>,
    failures: BTreeMap<u32, u8>,
    frequencies: BTreeMap<u32, f64>,
    thresholds: BTreeMap<u32, f64>,
    timestamps: BTreeMap<u32, Vec<u64>>,
    raw: BTreeMap<u32, Vec<u8>>,
    seen: Vec<u32>,
    last_nonempty: u64,
    last_update: u64,
    invalid_count: u8,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Value {
        value: f64,
    },
    Defined {
        definitions: openpilot_can::dbc::Definitions,
    },
    Packed {
        packed: Vec<u8>,
    },
    Update(Box<Snapshot>),
    Success {
        success: bool,
    },
    Error {
        error: String,
    },
}
#[derive(Serialize)]
struct Trace {
    dbc: Arc<Dbc>,
    steps: Vec<Output>,
}

fn trace(case: Case) -> Result<Trace, openpilot_can::Error> {
    let dbc = Arc::new(Dbc::load(&case.path)?);
    let mut parser = Parser::new(Arc::clone(&dbc), case.bus, case.now);
    for (name, frequency, ignore_counter) in case.messages {
        parser.add(&name, frequency, ignore_counter, case.now)?;
    }
    for name in case.ignored_messages {
        parser.add(&name, Some(f64::NAN), false, case.now)?;
    }
    let mut packer = Packer::new(Arc::clone(&dbc));
    let mut outputs = Vec::new();
    for step in case.steps {
        let output = match step {
            Step::Define => match dbc.definitions() {
                Ok(definitions) => Output::Defined { definitions },
                Err(error) => Output::Error {
                    error: error.to_string(),
                },
            },
            Step::Lazy { name, signal, now } => match parser.signal_lazy(&name, &signal, now) {
                Ok(value) => Output::Value { value },
                Err(error) => Output::Error {
                    error: error.to_string(),
                },
            },
            Step::Pack {
                address,
                values,
                rx_counter,
            } => {
                let values: Vec<_> = values
                    .iter()
                    .map(|(name, value)| (name.as_str(), *value))
                    .collect();
                match packer.pack_address(address, &values, rx_counter) {
                    Ok(packed) => Output::Packed { packed },
                    Err(error) => Output::Error {
                        error: error.to_string(),
                    },
                }
            }
            Step::Ready { enabled } => {
                parser.controls_ready = enabled;
                Output::Success { success: true }
            }
            Step::Add {
                name,
                frequency,
                ignore_counter,
                now,
            } => match parser.add(&name, frequency, ignore_counter, now) {
                Ok(()) => Output::Success { success: true },
                Err(error) => Output::Error {
                    error: error.to_string(),
                },
            },
            Step::Update { packets, checks } => match parser.update(&packets) {
                Err(error) => Output::Error {
                    error: error.to_string(),
                },
                Ok(updated) => {
                    let checks = (0..checks).map(|_| parser.can_valid()).collect();
                    Output::Update(Box::new(Snapshot {
                        updated: updated.into_iter().collect(),
                        checks,
                        bus_timeout: parser.bus_timeout(),
                        values: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.values.clone()))
                            .collect(),
                        all_values: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.all_values.clone()))
                            .collect(),
                        counters: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.counter.to_string()))
                            .collect(),
                        failures: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.counter_fail))
                            .collect(),
                        frequencies: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.frequency))
                            .collect(),
                        thresholds: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.timeout_threshold))
                            .collect(),
                        timestamps: parser
                            .states
                            .iter()
                            .map(|(id, state)| (*id, state.timestamps.iter().copied().collect()))
                            .collect(),
                        raw: parser.raw.clone(),
                        seen: parser.seen_addresses.iter().copied().collect(),
                        last_nonempty: parser.last_nonempty,
                        last_update: parser.last_update,
                        invalid_count: parser.invalid_count,
                    }))
                }
            },
        };
        outputs.push(output);
    }
    Ok(Trace {
        dbc,
        steps: outputs,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let traces: Vec<_> = request
        .cases
        .into_iter()
        .map(trace)
        .collect::<Result<_, _>>()?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&traces)?)?;
    Ok(())
}
