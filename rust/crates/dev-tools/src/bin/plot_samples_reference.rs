use std::io::{Read, Write};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    openpilot_dev_tools::plot_samples_check::validate_source(&root)?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let result = openpilot_dev_tools::plot_samples_source::render(
        &openpilot_log_format::JsonValue::parse(&input)?,
    )?;
    writeln!(std::io::stdout(), "{}", result.to_json()?)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
