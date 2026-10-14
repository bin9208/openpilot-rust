use std::path::PathBuf;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--binary")) {
        return Err("usage: check-filter-reference --binary FILTER_TRACE".into());
    }
    let binary = PathBuf::from(args.next().ok_or("--binary requires a path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let report = openpilot_dev_tools::filter_reference::check(&binary)?;
    println!(
        "PASS: {} scalar outputs match pinned original-source filters; max abs error={:.3e}",
        report.outputs, report.max_abs_error
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("filter reference: {error}");
        std::process::exit(1);
    }
}
