use openpilot_usbgpu::model_delivery::presence;
use std::{io, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = serde_json::from_reader(io::stdin().lock())?;
    serde_json::to_writer(io::stdout().lock(), &presence::present(&root))?;
    Ok(())
}
