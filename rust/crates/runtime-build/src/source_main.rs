use openpilot_runtime_build::{source_store, Result};
use std::path::PathBuf;

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut root, mut store, mut growth) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-source --root CHECKOUT --store SOURCE_STORE --expected-growth-bytes BYTES\nRetain a clean, committed local source and initialized submodules independently of the mutable updater checkout. Does not build or activate a runtime.");
                return Ok(());
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing source root")?));
            }
            Some("--store") if store.is_none() => {
                store = Some(PathBuf::from(args.next().ok_or("missing source store")?));
            }
            Some("--expected-growth-bytes") if growth.is_none() => {
                growth = Some(
                    args.next()
                        .ok_or("missing growth estimate")?
                        .to_str()
                        .ok_or("invalid growth estimate")?
                        .parse::<u64>()?,
                );
            }
            _ => return Err("unknown or duplicate source retention argument".into()),
        }
    }
    let snapshot = source_store::retain(
        &root.ok_or("--root is required")?,
        &store.ok_or("--store is required")?,
        growth.ok_or("--expected-growth-bytes is required")?,
    )?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime source: {error}");
        std::process::exit(1);
    }
}
