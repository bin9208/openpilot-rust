use openpilot_can::Frame;
use openpilot_card::{
    isotp,
    query::{DiagnosticLevel, Error, ParallelQuery, QueryConfig, QueryIo, Target},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{self, Read},
};

#[derive(Deserialize)]
struct Case {
    bus: u8,
    addrs: Vec<Target>,
    request: Vec<Vec<u8>>,
    response: Vec<Vec<u8>>,
    response_offset: i64,
    functional_addrs: Vec<u32>,
    response_pending_timeout: f64,
    timeout: f64,
    total_timeout: f64,
    batches: Vec<Vec<Vec<Frame>>>,
    clock_step: f64,
}

#[derive(Serialize)]
struct Io {
    #[serde(skip)]
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    delays: Vec<f64>,
    receives: Vec<bool>,
    diagnostics: Vec<(DiagnosticLevel, String)>,
    now: f64,
    #[serde(skip)]
    clock_step: f64,
}
impl QueryIo for Io {
    fn log(&mut self, level: DiagnosticLevel, message: &str) {
        self.diagnostics.push((level, message.to_owned()));
    }
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
enum Outcome {
    Data { data: Vec<(Target, Vec<u8>)> },
    Failure { error: &'static str, detail: String },
}
#[derive(Serialize)]
struct Output {
    result: Outcome,
    io: Io,
}
fn trace(case: Case) -> Output {
    let mut io = Io {
        batches: case.batches.into(),
        sent: Vec::new(),
        delays: Vec::new(),
        receives: Vec::new(),
        diagnostics: Vec::new(),
        now: 0.,
        clock_step: case.clock_step,
    };
    let query = ParallelQuery::new(QueryConfig {
        bus: case.bus,
        targets: &case.addrs,
        request: &case.request,
        response: &case.response,
        response_offset: case.response_offset,
        functional_addrs: &case.functional_addrs,
        response_pending_timeout: case.response_pending_timeout,
    });
    let result = match query
        .and_then(|mut query| query.get_data(case.timeout, case.total_timeout, &mut io))
    {
        Ok(data) => Outcome::Data { data },
        Err(error) => failure(error),
    };
    Output { result, io }
}
fn failure(error: Error) -> Outcome {
    Outcome::Failure {
        error: error.source_exception(),
        detail: error.to_string(),
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
