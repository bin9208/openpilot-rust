use openpilot_locationd::{wire, Error};
use openpilot_messaging::runtime::PubMaster;
use openpilot_msgq::Subscriber;
use serde::Deserialize;
use serde_json::json;
use std::{io::Write, time::Duration};

#[derive(Deserialize)]
struct Frame {
    packets: Vec<Vec<u8>>,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut publisher = PubMaster::for_runtime(&[
        "accelerometer",
        "gyroscope",
        "carState",
        "liveCalibration",
        "cameraOdometry",
    ])?;
    let capacity = openpilot_messaging::services::lookup("livePose")
        .ok_or(Error::Contract("livePose service"))?
        .queue_size;
    let mut subscriber = Subscriber::for_runtime("livePose", false, capacity)?;
    let _ = subscriber.receive(Duration::ZERO)?;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    for line in std::io::stdin().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        for packet in frame.packets {
            let event = wire::decode(&packet)?;
            publisher.send(event.service, &packet)?;
        }
        let packet = subscriber
            .receive(Duration::from_secs(5))?
            .ok_or(Error::Contract("livePose timeout"))?;
        println!("{}", json!({"packet":packet}));
        std::io::stdout().flush()?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()
}
