use openpilot_pandad::{
    can::{Decoder, Encoder},
    spi_alert::Tracker,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    io::{self, BufRead, Write},
};

#[derive(Deserialize)]
struct Frame {
    address: u32,
    src: u8,
    data: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Onroad {
        now: String,
        value: bool,
    },
    Observe {
        now: String,
        count: String,
        terminal: bool,
    },
    Ready {
        now: String,
    },
    Mark {
        now: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Pack { offset: u32, frames: Vec<Frame> },
    Decode { offset: u32, chunks: Vec<Vec<u8>> },
    Alerts { operations: Vec<Operation> },
}

fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    match request {
        Request::Pack { offset, frames } => {
            let mut chunks = Vec::new();
            let mut write = |bytes: &[u8]| {
                chunks.push(bytes.to_vec());
                Ok::<_, Infallible>(())
            };
            let mut encoder = Encoder::new(offset);
            for frame in frames {
                encoder.push(frame.address, frame.src, &frame.data, &mut write)?;
            }
            encoder.finish(&mut write)?;
            Ok(json!({"chunks": chunks}))
        }
        Request::Decode { offset, chunks } => {
            let mut decoder = Decoder::new(offset);
            let mut frames = Vec::new();
            let mut logs = Vec::new();
            let mut results = Vec::new();
            let mut resets = 0;
            for chunk in chunks {
                let ok = decoder.push(&chunk, &mut frames)?;
                if !ok {
                    resets += 1;
                    logs.push(json!({"level": 40, "message": "Panda CAN checksum failed"}));
                }
                results.push(json!({"ok": ok, "frames": frames, "remaining": decoder.remaining(), "resets": resets, "logs": logs}));
            }
            Ok(json!({"results": results}))
        }
        Request::Alerts { operations } => {
            let mut tracker = Tracker::default();
            let mut results = Vec::new();
            for operation in operations {
                let result = match operation {
                    Operation::Onroad { now, value } => {
                        tracker.update_onroad(value, now.parse()?);
                        Value::Null
                    }
                    Operation::Observe {
                        now,
                        count,
                        terminal,
                    } => tracker
                        .observe(now.parse()?, count.parse()?, terminal)
                        .into(),
                    Operation::Ready { now } => tracker.ready(now.parse()?).into(),
                    Operation::Mark { now } => {
                        let _: u64 = now.parse()?;
                        tracker.mark_capture_requested();
                        Value::Null
                    }
                };
                let mut state = serde_json::to_value(&tracker)?;
                state
                    .as_object_mut()
                    .ok_or("tracker serialization is not an object")?
                    .insert("result".into(), result);
                results.push(state);
            }
            Ok(json!({"results": results}))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request = serde_json::from_str(&line?)?;
        serde_json::to_writer(&mut output, &run(request)?)?;
        writeln!(output)?;
    }
    Ok(())
}
