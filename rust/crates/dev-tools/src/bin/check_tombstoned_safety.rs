use openpilot_dev_tools::{seal::Result, tombstone_safety_check as safety};
use std::path::{Path, PathBuf};

fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let environment = std::env::vars_os().collect();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let options = safety::parse(&args, &cwd, &environment)?;
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let report = safety::run(&root, &options)?;
    println!("{}", safety::format_report(&report)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        if let Some(safety::Interrupted(signal)) = error.downcast_ref() {
            let _ = signal_hook::low_level::emulate_default_handler(*signal);
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}
