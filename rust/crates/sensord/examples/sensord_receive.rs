mod support;
use openpilot_msgq::{MultiSubscriber, Subscription};
use openpilot_sensord::Error;
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .ok_or(Error::Contract("duration required"))?
        .parse()?;
    let specs = ["accelerometer", "gyroscope", "temperatureSensor"]
        .into_iter()
        .map(|name| {
            let service =
                openpilot_messaging::services::lookup(name).expect("known sensor service");
            Subscription {
                endpoint: name,
                capacity: service.queue_size,
                polled: true,
            }
        })
        .collect::<Vec<_>>();
    let mut subscriber = MultiSubscriber::new(&specs)?;
    println!("{{\"ready\":true}}");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        for message in subscriber.receive(Duration::from_millis(100))? {
            println!("{}", support::packet(&message.bytes)?);
        }
    }
    Ok(())
}
