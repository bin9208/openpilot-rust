use openpilot_dev_tools::{paramsd_startup_check, seal::Result};
use std::{collections::BTreeMap, path::PathBuf};

fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let mut options = BTreeMap::new();
    while let Some(flag) = arguments.next() {
        let flag = flag.into_string().map_err(|_| "non-UTF-8 option")?;
        if !matches!(flag.as_str(), "--root" | "--binary" | "--evidence") {
            return Err(format!("unknown option: {flag}").into());
        }
        let value = PathBuf::from(arguments.next().ok_or("option value missing")?);
        if options.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let mut take = |flag: &str| {
        options
            .remove(flag)
            .ok_or_else(|| format!("{flag} required"))
    };
    paramsd_startup_check::run(&paramsd_startup_check::Options {
        root: take("--root")?,
        binary: take("--binary")?,
        evidence: take("--evidence")?,
    })
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("paramsd startup native: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
