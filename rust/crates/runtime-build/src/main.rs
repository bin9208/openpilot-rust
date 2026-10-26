use openpilot_runtime_build::{build, Recipe, Result};
use std::{collections::BTreeMap, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if key == "--help" || key == "-h" {
            println!("Usage: openpilot-runtime-build --root SOURCE --recipe JSON --cache DIRECTORY --toolchain BIN_DIRECTORY\nBuild only, using a prepared native Rust 1.94.0 toolchain and offline Cargo dependencies. No activation or release download.");
            return Ok(());
        }
        if !["--root", "--recipe", "--cache", "--toolchain"]
            .iter()
            .any(|k| key == *k)
            || values.contains_key(&key)
        {
            return Err("unknown or duplicate build argument".into());
        }
        values.insert(
            key,
            PathBuf::from(args.next().ok_or("build argument needs a value")?),
        );
    }
    let take = |key: &str| {
        values
            .get(std::ffi::OsStr::new(key))
            .ok_or_else(|| format!("missing {key}"))
    };
    let recipe = Recipe::read(take("--recipe")?)?;
    let report = build(
        take("--root")?,
        take("--cache")?,
        take("--toolchain")?,
        &recipe,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime build: {error}");
        std::process::exit(1);
    }
}
