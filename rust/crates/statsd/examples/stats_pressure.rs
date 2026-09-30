use openpilot_statsd::producer::StatLog;
use std::io::{self, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args().nth(1).ok_or("endpoint")?;
    let mut producer = StatLog::new(endpoint);
    for index in 0..20_000 {
        producer.gauge("pressure", f64::from(index))?;
    }
    println!("ready");
    io::stdout().flush()?;
    let mut acknowledgement = String::new();
    if io::stdin().read_line(&mut acknowledgement)? == 0 {
        return Err("peer acknowledgement missing".into());
    }
    Ok(())
}
