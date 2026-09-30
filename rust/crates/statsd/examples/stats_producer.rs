use openpilot_statsd::producer::StatLog;
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args().nth(1).ok_or("endpoint")?;
    let mut producer = StatLog::new(endpoint);
    for line in io::stdin().lock().lines() {
        let metric = line?;
        println!("{:?}", producer.send(&metric)?);
        io::stdout().flush()?;
    }
    Ok(())
}
