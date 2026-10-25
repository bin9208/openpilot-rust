//! Private QA assets only: real PCM16 WAV fixtures, no audio device access.
use openpilot_dev_tools::{argparse_scan::ArgumentError, seal::Result};
use std::{ffi::OsString, path::Path};
fn execute(args: &[OsString]) -> Result<()> {
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h")) {
        println!("usage: create-soundd-fixture-assets ROOT RATE CHANNELS\nCreate twelve PCM16 WAV files; channels must be 1 or 2.");
        return Ok(());
    }
    let invalid = || {
        ArgumentError("usage: create-soundd-fixture-assets ROOT RATE CHANNELS; positive WAV-compatible rate and channels 1 or 2 required".into())
    };
    if args.len() != 3 {
        return Err(invalid().into());
    }
    let rate = args[1]
        .to_str()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    let channels = args[2]
        .to_str()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| matches!(n, 1 | 2))
        .ok_or_else(invalid)?;
    // wave's PCM16 byte-rate field is UInt32; reject invalid new CLI inputs
    // before create_assets can create even its first directory.
    rate.checked_mul(u32::from(channels) * 2)
        .ok_or_else(invalid)?;
    openpilot_dev_tools::soundd_policy_reference::create_assets(
        Path::new(&args[0]),
        rate,
        channels,
    )?;
    Ok(())
}
fn main() {
    if let Err(error) = execute(&std::env::args_os().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(if error.is::<ArgumentError>() { 2 } else { 1 });
    }
}
