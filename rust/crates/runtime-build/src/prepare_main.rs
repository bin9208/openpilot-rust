use openpilot_runtime_build::{prepare, Result};
use std::{collections::BTreeMap, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut paths = BTreeMap::new();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-prepare --root CHECKOUT --plan PLAN_JSON --cache CARGO_CACHE --toolchain NATIVE_RUST_BIN --source-store SOURCE_STORE --object-store OBJECT_STORE\nBuild serial recipe groups from one stable input path, retaining exact source and immutable output copies. Does not seal, activate or start a runtime.");
                return Ok(());
            }
            Some(
                key @ ("--root" | "--plan" | "--cache" | "--toolchain" | "--source-store"
                | "--object-store"),
            ) if !paths.contains_key(key) => {
                paths.insert(
                    key.to_owned(),
                    PathBuf::from(args.next().ok_or("missing prepare path")?),
                );
            }
            _ => return Err("unknown or duplicate native prepare argument".into()),
        }
    }
    let path = |key: &str| {
        paths
            .get(key)
            .map(PathBuf::as_path)
            .ok_or_else(|| format!("{key} is required"))
    };
    let plan = prepare::Plan::read(path("--plan")?)?;
    let prepared = prepare::run(
        path("--root")?,
        path("--cache")?,
        path("--toolchain")?,
        path("--source-store")?,
        path("--object-store")?,
        &plan,
    )?;
    println!("{}", serde_json::to_string_pretty(&prepared)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime prepare: {error}");
        std::process::exit(1);
    }
}
