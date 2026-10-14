use openpilot_dev_tools::{brotli_stage, seal::Result};
use std::path::PathBuf;
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("SDK_LIB_DIR required")?);
    let output = PathBuf::from(args.next().ok_or("OUTPUT required")?);
    let target = args.next().ok_or("TARGET required")?;
    let version = args.next().ok_or("VERSION required")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let report = brotli_stage::stage(
        &source,
        &output,
        target.to_str().ok_or("invalid target")?,
        version.to_str().ok_or("invalid version")?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Brotli staging: {error}");
        std::process::exit(1);
    }
}
