use openpilot_cereal::log_capnp::event;
use openpilot_lagd::loop_state::TOPICS;
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Cursor, Write},
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Request {
    messages: Vec<Vec<u8>>,
    receive: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut publisher = PubMaster::for_runtime(&TOPICS)?;
    let mut subscriber = SubMaster::for_runtime(&["liveDelay"], Options::default())?;
    println!("ready");
    std::io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let row: Request = serde_json::from_str(&line?)?;
        let mut topics = Vec::new();
        for bytes in &row.messages {
            let message = capnp::serialize::read_message(
                Cursor::new(bytes),
                capnp::message::ReaderOptions::new(),
            )?;
            let value = message.get_root::<event::Reader>()?;
            let topic = match value.which()? {
                event::LivePose(_) => "livePose",
                event::LiveCalibration(_) => "liveCalibration",
                event::CarState(_) => "carState",
                event::ControlsState(_) => "controlsState",
                event::CarControl(_) => "carControl",
                _ => return Err("unknown fixture service".into()),
            };
            publisher.send(topic, bytes)?;
            topics.push(topic);
        }
        for topic in topics {
            if !publisher.wait_for_readers(
                topic,
                Duration::from_secs(5),
                Duration::from_millis(1),
            )? {
                return Err("daemon did not consume frame".into());
            }
        }
        let mut packet = None;
        if row.receive {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                subscriber.update(Duration::from_millis(100))?;
                let topic = subscriber.state.topic("liveDelay")?;
                if topic.updated {
                    let mut value = capnp::message::Builder::new_default();
                    value.set_root(topic.event()?)?;
                    packet = Some(capnp::serialize::write_message_to_words(&value));
                    break;
                }
            }
            if packet.is_none() {
                return Err("liveDelay timeout".into());
            }
        }
        println!("{}", json!({"packet":packet}));
        std::io::stdout().flush()?;
    }
    Ok(())
}
