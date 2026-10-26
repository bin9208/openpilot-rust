use openpilot_runtime_build::{catalog, Result};
use std::path::PathBuf;

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut root, mut cargo, mut bindings, mut output) = (None, None, None, None);
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-plan --root CHECKOUT --cargo ABSOLUTE_CARGO (--check | --bindings PINNED_INPUTS_JSON --output NEW_DIRECTORY)\nAudit the tracked ARM runtime roster against actual locked Cargo metadata. Assemble its existing serial build plan from verified input receipts and explicit per-group assignments. No compilation, dependency download, generation sealing or activation is performed.");
                return Ok(());
            }
            Some("--check") if !check => check = true,
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing root")?))
            }
            Some("--cargo") if cargo.is_none() => {
                cargo = Some(PathBuf::from(args.next().ok_or("missing Cargo")?))
            }
            Some("--bindings") if bindings.is_none() => {
                bindings = Some(PathBuf::from(args.next().ok_or("missing bindings")?))
            }
            Some("--output") if output.is_none() => {
                output = Some(PathBuf::from(args.next().ok_or("missing output")?))
            }
            _ => return Err("unknown or duplicate native plan argument".into()),
        }
    }
    let root = root.ok_or("--root is required")?;
    let cargo = cargo.ok_or("--cargo is required")?;
    let audit = if check && bindings.is_none() && output.is_none() {
        catalog::audit(&root, &cargo)?.1
    } else if !check {
        let bindings = bindings.ok_or("--bindings is required for assembly")?;
        let output = output.ok_or("--output is required for assembly")?;
        let (plan, audit) = catalog::assemble(&root, &cargo, &bindings)?;
        catalog::publish(&output, &plan, &audit)?;
        audit
    } else {
        return Err("choose --check or assembly with --bindings and --output".into());
    };
    println!("{}", serde_json::to_string_pretty(&audit)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native runtime plan: {error}");
        std::process::exit(1);
    }
}
