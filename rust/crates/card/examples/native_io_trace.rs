use openpilot_can::Frame;
use openpilot_card::{core::StepIo, firmware_query::StartupIo, query::QueryIo, runtime::NativeIo};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Startup,
    Receive { wait: bool },
    Update,
    Send { frames: Vec<Frame> },
    Obd { enabled: bool },
    Quit,
}

fn report(value: Value) -> io::Result<()> {
    println!("CARD_TRACE {value}");
    io::stdout().flush()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut io = NativeIo::new(Params::for_runtime()?, stop)?;
    report(json!({"ready": true}))?;
    for line in io::stdin().lock().lines() {
        let command: Command = serde_json::from_str(&line?)?;
        let result = match command {
            Command::Startup => {
                json!({"pandas": io.wait_for_startup()?, "frame": io.subscribers().frame()})
            }
            Command::Receive { wait } => match io.receive(wait) {
                Ok(packets) => json!({"packets": packets}),
                Err(error) => json!({"error": error.source_exception()}),
            },
            Command::Update => {
                io.update_subscribers()?;
                let topics: Vec<_> = io.subscribers().topics().iter().map(|topic| {
                    let mut message = capnp::message::Builder::new_default();
                    message.set_root(topic.event()?)?;
                    let bytes = capnp::serialize::write_message_to_words(&message);
                    Ok::<_, Box<dyn std::error::Error>>(json!({"name": topic.service.name, "seen": topic.seen, "updated": topic.updated,
                        "alive": topic.alive, "valid": topic.valid, "frame": topic.receive_frame, "mono": topic.log_mono_time, "event": bytes}))
                }).collect::<Result<_, _>>()?;
                json!({"frame": io.subscribers().frame(), "topics": topics})
            }
            Command::Send { frames } => {
                io.send(&frames)?;
                json!({"sent": frames.len()})
            }
            Command::Obd { enabled } => {
                io.set_obd_multiplexing(enabled)?;
                json!({"obd": enabled})
            }
            Command::Quit => break,
        };
        report(result)?;
    }
    Ok(())
}
