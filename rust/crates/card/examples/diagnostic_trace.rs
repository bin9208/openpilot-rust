use openpilot_can::Frame;
use openpilot_card::{
    ecu::{self, DisableConfig, EcuAddress, ScanConfig},
    isotp,
    query::QueryIo,
    vin::{self, VinConfig},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{self, Read},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Case {
    Decode {
        bytes: Vec<u8>,
    },
    Valid {
        vin: String,
    },
    Scan {
        queries: Vec<EcuAddress>,
        responses: Vec<EcuAddress>,
        timeout: f64,
        io: Input,
    },
    Disable {
        target: EcuAddress,
        request: Vec<u8>,
        timeout: f64,
        retry: usize,
        io: Input,
    },
    Vin {
        buses: Vec<u8>,
        timeout: f64,
        retry: usize,
        io: Input,
    },
}
#[derive(Default, Deserialize)]
struct Input {
    batches: Vec<Vec<Vec<Frame>>>,
    clock_step: f64,
}
#[derive(Default, Serialize)]
struct Io {
    #[serde(skip)]
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    delays: Vec<f64>,
    receives: Vec<bool>,
    now: f64,
    #[serde(skip)]
    clock_step: f64,
}
impl From<Input> for Io {
    fn from(input: Input) -> Self {
        Self {
            batches: input.batches.into(),
            clock_step: input.clock_step,
            ..Self::default()
        }
    }
}
impl QueryIo for Io {
    fn receive(&mut self, wait_for_one: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        self.receives.push(wait_for_one);
        Ok(self.batches.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), isotp::Error> {
        self.sent.extend_from_slice(frames);
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), isotp::Error> {
        self.delays.push(seconds);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now += self.clock_step;
        self.now
    }
}
#[derive(Serialize)]
#[serde(untagged)]
enum ResultValue {
    Decode(Option<String>),
    Bool(bool),
    Scan(Vec<EcuAddress>),
    Vin(i64, i16, String),
}
#[derive(Serialize)]
struct Output {
    result: ResultValue,
    io: Io,
}
fn trace(case: Case) -> Output {
    match case {
        Case::Decode { bytes } => Output {
            result: ResultValue::Decode(vin::decode(&bytes)),
            io: Io::default(),
        },
        Case::Valid { vin: text } => Output {
            result: ResultValue::Bool(vin::valid(&text)),
            io: Io::default(),
        },
        Case::Scan {
            queries,
            responses,
            timeout,
            io,
        } => {
            let mut io = Io::from(io);
            let mut results = ecu::scan(
                ScanConfig {
                    queries: &queries,
                    responses: &responses,
                    timeout,
                },
                &mut io,
            );
            results.sort_unstable();
            Output {
                result: ResultValue::Scan(results),
                io,
            }
        }
        Case::Disable {
            target,
            request,
            timeout,
            retry,
            io,
        } => {
            let mut io = Io::from(io);
            let result = ecu::disable(
                DisableConfig {
                    target,
                    communication_request: &request,
                    timeout,
                    retry,
                },
                &mut io,
            );
            Output {
                result: ResultValue::Bool(result),
                io,
            }
        }
        Case::Vin {
            buses,
            timeout,
            retry,
            io,
        } => {
            let mut io = Io::from(io);
            let result = vin::query(
                VinConfig {
                    buses: &buses,
                    timeout,
                    retry,
                },
                &mut io,
            );
            Output {
                result: ResultValue::Vin(
                    result.address.map_or(-1, i64::from),
                    result.bus.map_or(-1, i16::from),
                    result.vin,
                ),
                io,
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output: Vec<_> = cases.into_iter().map(trace).collect();
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&output)?)?;
    Ok(())
}
