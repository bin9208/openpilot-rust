//! Actions-only consumer of the independently source-bound original DBC corpus.
use openpilot_radarcan::databases::Databases;
use std::{collections::BTreeSet, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("DBC directory required")?);
    if args.next().is_some() {
        return Err("only the DBC directory is accepted".into());
    }
    let names = std::fs::read_dir(&root)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "dbc"))
        .map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
                .ok_or("non-UTF8 DBC name")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if names.len() != 103 {
        return Err("incomplete original DBC corpus".into());
    }
    let mut databases = Databases::new(root);
    let mut messages = 0;
    for name in &names {
        messages += databases.get(name, &mut |_| {})?.messages.len();
    }
    println!(
        "{}",
        serde_json::json!({"databases":names.len(),"messages":messages,"runtime_activated":false})
    );
    Ok(())
}
