use openpilot_runtime_build::{catalog, catalog_bindings, Result};
use std::path::PathBuf;

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut root, mut cargo, mut bindings, mut output) = (None, None, None, None);
    let (mut native_inputs, mut budgets) = (None, None);
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-plan --root CHECKOUT --cargo ABSOLUTE_CARGO (--check | --bindings BINDINGS_JSON --output NEW_DIRECTORY | --native-inputs PREPARED_PINS_JSON --budgets GROWTH_ESTIMATES_JSON --output NEW_DIRECTORY)\nAudit the tracked ARM runtime roster and compiler input requirements against actual locked Cargo metadata. Native input assembly verifies independently pinned prepared Trees and the source's complete system SDK profile, assigns each provider only to consuming groups, and publishes bindings.json, build-plan.json and catalog-audit.json atomically. Growth estimates require an explicit basis. compiler_inputs_bound reports compiler coverage; runtime assets and generation sealing remain separate gates. Incomplete engineering plans cannot pass full generation validation. No compilation, dependency download, sealing or activation is performed.");
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
            Some("--native-inputs") if native_inputs.is_none() => {
                native_inputs = Some(PathBuf::from(args.next().ok_or("missing native inputs")?))
            }
            Some("--budgets") if budgets.is_none() => {
                budgets = Some(PathBuf::from(args.next().ok_or("missing budgets")?))
            }
            Some("--output") if output.is_none() => {
                output = Some(PathBuf::from(args.next().ok_or("missing output")?))
            }
            _ => return Err("unknown or duplicate native plan argument".into()),
        }
    }
    let root = root.ok_or("--root is required")?;
    let cargo = cargo.ok_or("--cargo is required")?;
    let audit = match (check, bindings, native_inputs, budgets, output) {
        (true, None, None, None, None) => catalog::audit(&root, &cargo)?.1,
        (false, None, Some(inputs), Some(budgets), Some(output)) => {
            let (bindings, plan, audit) =
                catalog_bindings::assemble(&root, &cargo, &inputs, &budgets, &output)?;
            catalog::publish_with_bindings(&output, &plan, &audit, Some(&bindings))?;
            audit
        }
        (false, Some(bindings), None, None, Some(output)) => {
            let (plan, audit) = catalog::assemble(&root, &cargo, &bindings)?;
            catalog::publish(&output, &plan, &audit)?;
            audit
        }
        _ => {
            return Err(
                "choose --check, --bindings/--output, or --native-inputs/--budgets/--output".into(),
            )
        }
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
