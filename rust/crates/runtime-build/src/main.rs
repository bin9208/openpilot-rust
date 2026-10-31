use openpilot_runtime_build::{build_with_metadata, Recipe, Result};
use std::{collections::BTreeMap, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    let mut metadata = false;
    while let Some(key) = args.next() {
        if key == "--cargo-metadata" && !metadata {
            metadata = true;
            continue;
        }
        if key == "--help" || key == "-h" {
            println!("Usage: openpilot-runtime-build --root SOURCE --recipe JSON --cache DIRECTORY --toolchain BIN_DIRECTORY [--cargo-metadata]\nBuild only, using a prepared native Rust 1.94.0 toolchain and offline Cargo dependencies. --cargo-metadata returns the unchanged build report together with actual Cargo executable feature/profile messages. No activation or release download.");
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
    let report = build_with_metadata(
        take("--root")?,
        take("--cache")?,
        take("--toolchain")?,
        &recipe,
    )?;
    if metadata {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&report.build)?);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime build: {error}");
        std::process::exit(1);
    }
}
