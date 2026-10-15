use openpilot_dev_tools::{
    publish::{publish_for_branch, GhGithub},
    seal::Result,
};
use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    let mut check = false;
    let mut branch = None;
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid argument")?;
        match key {
            "--branch" => {
                let value = args.next().ok_or("missing branch")?;
                let value = value.into_string().map_err(|_| "invalid branch")?;
                if branch.replace(value).is_some() {
                    return Err("duplicate --branch".into());
                }
            }
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
                    "Usage: publish-runtime-release --root ROOT --assets DIRECTORY [--branch dev|dev-md] [--check-only]"
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
    let result = publish_for_branch(
        &root,
        &assets,
        &mut GhGithub,
        check,
        branch.as_deref().unwrap_or("dev"),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("runtime publication: {error}");
        std::process::exit(1);
    }
}
