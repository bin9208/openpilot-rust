use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use openpilot_msgq::Subscriber;
use serde::Deserialize;
use serde_json::json;
use std::{io::Write, time::Duration};

#[derive(Deserialize)]
struct Frame {
    packets: Vec<Vec<u8>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gps = std::env::args().nth(1).ok_or("GPS service")?;
    let mut publisher = PubMaster::for_runtime(&["carState", "liveCalibration", &gps, "livePose"])?;
    let capacity = openpilot_messaging::services::lookup("liveParameters")
        .ok_or("service")?
        .queue_size;
    let mut subscriber = Subscriber::for_runtime("liveParameters", false, capacity)?;
    let _ = subscriber.receive(Duration::ZERO)?;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for line in std::io::stdin().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        for packet in frame.packets {
            let message = serialize::read_message(packet.as_slice(), ReaderOptions::new())?;
            let root = message.get_root::<event::Reader<'_>>()?;
            let service = match root.which()? {
                event::CarState(_) => "carState",
                event::LiveCalibration(_) => "liveCalibration",
                event::GpsLocation(_) => "gpsLocation",
                event::GpsLocationExternal(_) => "gpsLocationExternal",
                event::LivePose(_) => "livePose",
                _ => return Err("unexpected input service".into()),
            };
            publisher.send(service, &packet)?;
        }
        let packet = subscriber
            .receive(Duration::from_secs(5))?
            .ok_or("liveParameters timeout")?;
        println!("{}", json!({"packet":packet}));
        std::io::stdout().flush()?;
    }
    Ok(())
}
