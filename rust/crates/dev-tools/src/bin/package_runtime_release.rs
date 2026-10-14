use openpilot_dev_tools::package;
use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    let mut verify = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid argument")?;
        match key {
            "--verify-only" => {
                if verify {
                    return Err("duplicate --verify-only".into());
                }
                verify = true;
            }
            "--root" | "--installer" | "--installer-build-receipt" | "--output" => {
                let value = args.next().ok_or("missing argument value")?;
                if values
                    .insert(key.to_owned(), PathBuf::from(value))
                    .is_some()
                {
                    return Err("duplicate argument".into());
                }
            }
            "--help" | "-h" => {
                println!("Usage: package-runtime-release --root ROOT --output DIRECTORY [--installer ELF --installer-build-receipt JSON | --verify-only]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {key}").into()),
        }
    }
    let root = values.get("--root").ok_or("--root required")?;
    let output = values.get("--output").ok_or("--output required")?;
    let v = if verify {
        package::validate_assets(root, output)?
    } else {
        package::package(
            root,
            values
                .get("--installer")
                .ok_or("installer and build receipt are required")?,
            values
                .get("--installer-build-receipt")
                .ok_or("installer and build receipt are required")?,
            output,
        )?
    };
    println!("{}", serde_json::to_string(&v)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("runtime release: {e}");
        std::process::exit(1);
    }
}
