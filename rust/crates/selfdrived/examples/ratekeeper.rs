use openpilot_selfdrived::runtime::clock::Ratekeeper;
use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rate = Ratekeeper::default();
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let values: Vec<f64> = serde_json::from_str(&line?)?;
        let expected = if rate.last_monitor_time < 0.0 { 4 } else { 2 };
        if values.len() != expected {
            return Err("wrong monotonic call count".into());
        }
        let lagging_before = rate.lagging();
        let mut index = 0;
        let lagged = rate.monitor_time(|| {
            let value = values[index];
            index += 1;
            value
        });
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({"rate":rate,"lagged":lagged,"lagging_before":lagging_before,"lagging_after":rate.lagging()}),
        )?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
