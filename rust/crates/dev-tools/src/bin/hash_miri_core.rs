use std::io::Write;
fn run() -> openpilot_dev_tools::seal::Result<()> {
    let root = std::env::var_os("MIRI_SYSROOT").ok_or("MIRI_SYSROOT is not set")?;
    let output = openpilot_dev_tools::miri_provenance::core_hashes(&root)?;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
