use std::io::Write;

fn run() -> openpilot_dev_tools::seal::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: list-cargo-tests JSONL")?;
    let output = openpilot_dev_tools::ci_artifact_list::select(&std::fs::read(path)?)?;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
