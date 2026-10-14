use openpilot_dev_tools::{
    publish::{publish, GhGithub},
    seal::Result,
};
use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    let mut check = false;
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid argument")?;
        match key {
            "--check-only" => {
                if check {
                    return Err("duplicate --check-only".into());
                }
                check = true;
            }
            "--root" | "--assets" => {
                let value = args.next().ok_or("missing argument value")?;
                if values
                    .insert(key.to_owned(), PathBuf::from(value))
                    .is_some()
                {
                    return Err("duplicate argument".into());
                }
            }
            "--help" | "-h" => {
                println!(
                    "Usage: publish-runtime-release --root ROOT --assets DIRECTORY [--check-only]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {key}").into()),
        }
    }
    let root = values
        .get("--root")
        .ok_or("--root required")?
        .canonicalize()?;
    let assets = values
        .get("--assets")
        .ok_or("--assets required")?
        .canonicalize()?;
    let result = publish(&root, &assets, &mut GhGithub, check)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("runtime publication: {error}");
        std::process::exit(1);
    }
}
