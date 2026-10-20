use std::{collections::BTreeMap, ffi::OsStr, path::PathBuf};

fn run() -> openpilot_dev_tools::seal::Result<()> {
    let mut values = BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            println!("Usage: stage-eigen-headers --wheel PINNED_WHEEL --output NEW_DIRECTORY --architecture x86_64|aarch64|macos-arm64");
            return Ok(());
        }
        if !["--wheel", "--output", "--architecture"].contains(&flag.to_str().unwrap_or("")) {
            return Err("unknown Eigen staging argument".into());
        }
        let value = args.next().ok_or("missing Eigen staging argument value")?;
        if values.insert(flag, value).is_some() {
            return Err("duplicate Eigen staging argument".into());
        }
    }
    let wheel = PathBuf::from(
        values
            .remove(OsStr::new("--wheel"))
            .ok_or("--wheel required")?,
    );
    let output = PathBuf::from(
        values
            .remove(OsStr::new("--output"))
            .ok_or("--output required")?,
    );
    let architecture = values
        .remove(OsStr::new("--architecture"))
        .ok_or("--architecture required")?
        .into_string()
        .map_err(|_| "non-UTF8 Eigen architecture")?;
    let result = openpilot_dev_tools::eigen_stage::stage(&wheel, &output, &architecture)?;
    println!(
        "{}",
        serde_json::json!({"output": output, "architecture": result.architecture,
            "files": result.files.len(), "wheel_sha256": result.wheel_sha256})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Eigen header staging: {error}");
        std::process::exit(1);
    }
}
