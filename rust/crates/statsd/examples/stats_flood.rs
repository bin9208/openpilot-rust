//! Sustained real PUSH traffic for daemon shutdown verification.
use openpilot_statsd::producer::StatLog;
use std::io::{self, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut producer = StatLog::new(std::env::args().nth(1).ok_or("endpoint")?);
    for _ in 0..20_000 {
        producer.send("flood:1|g")?;
    }
    println!("ready");
    io::stdout().flush()?;
    loop {
        producer.send("flood:1|g")?;
    }
}
