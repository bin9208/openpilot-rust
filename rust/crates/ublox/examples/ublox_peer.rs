use openpilot_msgq::{MultiSubscriber, Subscription};
use openpilot_ublox::Error;
use serde_json::json;
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn main() -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let topics = ["ubloxRaw", "ubloxGnss", "gpsLocationExternal"];
    let specs: Vec<_> = topics
        .iter()
        .map(|name| {
            let service =
                openpilot_messaging::services::lookup(name).ok_or(Error::Malformed("service"))?;
            Ok(Subscription {
                endpoint: name,
                capacity: service.queue_size,
                polled: true,
            })
        })
        .collect::<Result<_, Error>>()?;
    let mut subscriber = MultiSubscriber::queued_for_runtime(&specs)?;
    println!("{{\"ready\":true}}");
    std::io::stdout().flush()?;
    while !stop.load(Ordering::Relaxed) {
        for packet in subscriber.receive(Duration::from_millis(100))? {
            println!("{}", json!({"packet":packet.bytes}));
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
