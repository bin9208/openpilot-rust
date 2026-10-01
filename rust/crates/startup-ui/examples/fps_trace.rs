use openpilot_startup_ui::diagnostics::fps::{Monitor, Sample};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples: Vec<Sample> = serde_json::from_reader(std::io::stdin())?;
    let mut monitor = Monitor::new(20, 0.0);
    let output: Vec<_> = samples
        .into_iter()
        .map(|sample| monitor.observe(sample))
        .collect();
    serde_json::to_writer(std::io::stdout(), &output)?;
    Ok(())
}
