use openpilot_dev_tools::{
    sdk_transport_check, seal::Result, tombstone_reference_check as reference,
};
use std::path::{Path, PathBuf};

fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let environment = std::env::vars_os().collect();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("SDK transport requires BINARY PARAMS_BINDING OUTPUT [RUNNER ARGS...]".into());
    }
    let reference::Command::Compare(options) = reference::parse(&args, &cwd, &environment)? else {
        return Err("SDK transport requires candidate and output paths".into());
    };
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let report = sdk_transport_check::run(&root, &options)?;
    println!("{}", reference::format_report(&report)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        if let Some(reference::Interrupted(signal)) = error.downcast_ref() {
            let _ = signal_hook::low_level::emulate_default_handler(*signal);
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}
