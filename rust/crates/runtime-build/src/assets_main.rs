use openpilot_runtime_build::{assets, Result};
use std::path::PathBuf;

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut plan = None;
    let mut cache = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-assets --plan JSON --cache DIRECTORY\nPrepare content-verified build inputs. Reuses verified objects without HTTP transfers. Does not extract, build or activate a runtime.");
                return Ok(());
            }
            Some("--plan") if plan.is_none() => {
                plan = Some(PathBuf::from(args.next().ok_or("missing plan path")?));
            }
            Some("--cache") if cache.is_none() => {
                cache = Some(PathBuf::from(args.next().ok_or("missing cache path")?));
            }
            _ => return Err("unknown or duplicate asset argument".into()),
        }
    }
    let plan = assets::Plan::read(&plan.ok_or("--plan is required")?)?;
    let receipts = assets::prepare(&cache.ok_or("--cache is required")?, &plan, &assets::Curl)?;
    println!("{}", serde_json::to_string_pretty(&receipts)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime assets: {error}");
        std::process::exit(1);
    }
}
