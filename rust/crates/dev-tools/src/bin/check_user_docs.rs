use openpilot_dev_tools::user_docs::{check, Options, Result};
use std::{io, path::PathBuf};

fn run() -> Result<bool> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let mut options = Options::new(root);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--commit-overrides" {
            options.commit_overrides = true;
            continue;
        }
        if arg == "--help" || arg == "-h" {
            println!("check-user-docs [--base REF] [--head REF] [--map PATH] [--repo PATH] [--commit-overrides | --changed-file PATH ...]");
            return Ok(true);
        }
        if !["--base", "--head", "--map", "--repo", "--changed-file"].contains(&arg.as_str()) {
            return Err(format!("unknown argument: {arg}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{arg} requires a value"))?;
        match arg.as_str() {
            "--base" => options.base = value,
            "--head" => options.head = value,
            "--map" => options.map = value.into(),
            "--repo" => options.root = value.into(),
            "--changed-file" => options.changed.push(value),
            _ => unreachable!(),
        }
    }
    check(
        &options,
        &std::env::var("PR_BODY").unwrap_or_default(),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("check-user-docs: {error}");
            std::process::exit(2);
        }
    }
}
