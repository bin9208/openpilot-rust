use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let parts: [i64; 2] = serde_json::from_str(&line?)?;
        let time = rustix::time::Timespec {
            tv_sec: parts[0],
            tv_nsec: parts[1],
        };
        if std::env::args().nth(1).as_deref() == Some("--monotonic") {
            println!(
                "{}",
                openpilot_statsd::clock::monotonic_seconds(time)?.to_bits()
            );
        } else {
            println!("{}", openpilot_statsd::clock::timestamp_ns(time)?);
        }
    }
    Ok(())
}
