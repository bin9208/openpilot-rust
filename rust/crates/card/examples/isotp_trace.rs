use openpilot_can::Frame;
use openpilot_card::isotp::{rx_address, CanClient, CanIo, Error, IsoTpMessage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{self, Read},
};

#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
    addresses: Vec<(u32, i64)>,
}
#[derive(Deserialize)]
struct Case {
    tx_addr: u32,
    rx_addr: Option<i64>,
    bus: u8,
    sub_addr: Option<u8>,
    rx_sub_addr: Option<u8>,
    timeout: f64,
    single_frame_mode: bool,
    separation_time: f64,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    op: Op,
    #[serde(default)]
    data: Vec<u8>,
    #[serde(default)]
    setup_only: bool,
    #[serde(default)]
    batches: Vec<Vec<Frame>>,
    timeout: Option<f64>,
    clock_step: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Op {
    Send,
    Receive,
    Drain,
}
#[derive(Default, Serialize)]
struct Io {
    #[serde(skip)]
    batches: VecDeque<Vec<Frame>>,
    sent: Vec<Frame>,
    delays: Vec<f64>,
    receives: usize,
    now: f64,
    #[serde(skip)]
    clock_step: f64,
}
impl CanIo for Io {
    fn receive(&mut self) -> Result<Vec<Frame>, Error> {
        self.receives += 1;
        Ok(self.batches.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frame: Frame) -> Result<(), Error> {
        self.sent.push(frame);
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.delays.push(seconds);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now += self.clock_step;
        self.now
    }
}
fn failure(error: &Error) -> Value {
    json!({"error": error.source_exception(), "detail": error.to_string()})
}
fn trace(case: Case) -> Result<Value, serde_json::Error> {
    let client = CanClient::new(
        case.tx_addr,
        case.rx_addr,
        case.bus,
        case.sub_addr,
        case.rx_sub_addr,
    );
    let mut state = match IsoTpMessage::new(
        client,
        case.timeout,
        case.single_frame_mode,
        case.separation_time,
    ) {
        Ok(state) => state,
        Err(error) => return Ok(failure(&error)),
    };
    let mut io = Io::default();
    let mut steps = Vec::new();
    for step in case.steps {
        io.batches.extend(step.batches);
        io.clock_step = step.clock_step;
        let result = match step.op {
            Op::Send => state
                .send(&step.data, step.setup_only, &mut io)
                .map(|()| json!({"success": true})),
            Op::Receive => state
                .recv(step.timeout, &mut io)
                .map(|(data, in_progress)| json!({"data": data, "in_progress": in_progress})),
            Op::Drain => state
                .client
                .receive_buffer(true, &mut io)
                .map(|()| json!({"success": true})),
        };
        let result = match result {
            Ok(value) => value,
            Err(error) => failure(&error),
        };
        steps.push(json!({"result": result, "state": serde_json::to_value(&state)?, "io": serde_json::to_value(&io)?}));
    }
    Ok(json!({"steps": steps}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let cases: Vec<_> = request
        .cases
        .into_iter()
        .map(trace)
        .collect::<Result<_, _>>()?;
    let addresses: Vec<_> = request
        .addresses
        .into_iter()
        .map(|(tx, offset)| match rx_address(tx, offset) {
            Ok(value) => json!({"value": value}),
            Err(error) => failure(&error),
        })
        .collect();
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(
        path,
        serde_json::to_vec(&json!({"cases": cases, "addresses": addresses}))?,
    )?;
    Ok(())
}
