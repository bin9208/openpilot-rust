use openpilot_messaging::{runtime::SubMaster, state::Options};
use std::{
    io::Write,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut subscriber = SubMaster::for_runtime(&["qcomGnss", "gpsLocation"], Options::default())?;
    println!("ready");
    std::io::stdout().flush()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        subscriber.update(Duration::from_millis(100))?;
        for topic in subscriber
            .state
            .topics()
            .iter()
            .filter(|topic| topic.updated)
        {
            let mut message = capnp::message::Builder::new_default();
            message.set_root(topic.event()?)?;
            println!(
                "{}",
                serde_json::json!({"topic":topic.service.name,"data":capnp::serialize::write_message_to_words(&message)})
            );
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}
