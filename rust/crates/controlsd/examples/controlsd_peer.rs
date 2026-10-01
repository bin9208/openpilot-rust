use capnp::{dynamic_value, message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use openpilot_msgq::Subscriber;
use serde::Deserialize;
use serde_json::json;
use std::{io::Write, time::Duration};
#[derive(Deserialize)]
struct Request {
    packets: Vec<Vec<u8>>,
    count: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut publisher = PubMaster::for_runtime(&openpilot_controlsd::SERVICES)?;
    let mut subscribers = Vec::new();
    for name in ["controlsState", "carControl"] {
        let mut subscriber = Subscriber::for_runtime(
            name,
            false,
            openpilot_messaging::services::lookup(name)
                .ok_or("service")?
                .queue_size,
        )?;
        let _ = subscriber.receive(Duration::ZERO)?;
        subscribers.push(subscriber);
    }
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for line in std::io::stdin().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        for subscriber in &mut subscribers {
            let _ = subscriber.receive(Duration::ZERO)?;
        }
        for packet in request.packets {
            let message = serialize::read_message(packet.as_slice(), ReaderOptions::new())?;
            let root = message.get_root::<event::Reader<'_>>()?;
            let dynamic_value::Reader::Struct(event) = root.into() else {
                return Err("event".into());
            };
            let field = event.which()?.ok_or("service")?;
            publisher.send(field.get_proto().get_name()?.to_str()?, &packet)?;
        }
        for _ in 0..request.count {
            let mut packets = Vec::new();
            for subscriber in &mut subscribers {
                packets.push(
                    subscriber
                        .receive(Duration::from_secs(5))?
                        .ok_or("publication timeout")?,
                );
            }
            println!("{}", json!({"packets":packets}));
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
