use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    let mut stage = false;
    while let Some(key) = arguments.next() {
        let key = key.into_string().map_err(|_| "non-UTF8 argument name")?;
        if key == "--stage" {
            if stage {
                return Err("duplicate --stage".into());
            }
            stage = true;
            continue;
        }
        if key == "--help" || key == "-h" {
            println!("Usage: build-runtime-panda --root ROOT --output OUTPUT [--toolchain ROOT | --compiler ROOT] [--jobs 1|2] [--stage]");
            return Ok(());
        }
        if !["--root", "--output", "--toolchain", "--compiler", "--jobs"].contains(&key.as_str()) {
            return Err("unknown argument".into());
        }
        let value = arguments.next().ok_or("argument needs a value")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate argument".into());
        }
    }
    let root = PathBuf::from(values.remove("--root").ok_or("--root required")?);
    let output = PathBuf::from(values.remove("--output").ok_or("--output required")?);
    let jobs = values
        .remove("--jobs")
        .map(|v| {
            v.into_string()
                .map_err(|_| "invalid jobs")
                .and_then(|v| v.parse::<u32>().map_err(|_| "invalid jobs"))
        })
        .transpose()?
        .unwrap_or(2);
    if !(1..=2).contains(&jobs) {
        return Err("jobs must be one or two".into());
    }
    let compiler = values.remove("--compiler");
    let toolchain = values.remove("--toolchain");
    if compiler.is_some() && toolchain.is_some() {
        return Err("duplicate toolchain selection".into());
    }
    if stage {
        openpilot_dev_tools::panda_build::stage(&root, &output)
    } else {
        let toolchain = PathBuf::from(
            toolchain
                .or(compiler)
                .ok_or("--toolchain required for build")?,
        );
        openpilot_dev_tools::panda_build::build(&root, &output, &toolchain, jobs)
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("build runtime Panda: {error}");
        std::process::exit(1);
    }
}
