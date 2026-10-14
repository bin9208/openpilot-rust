use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(key) = args.next() {
        let key = key.into_string().map_err(|_| "non-UTF8 argument name")?;
        if !["--wheel", "--output", "--lock"].contains(&key.as_str()) {
            return Err(format!("unknown argument: {key}").into());
        }
        let value = args.next().ok_or("argument requires a value")?;
        if values.insert(key, PathBuf::from(value)).is_some() {
            return Err("duplicate argument".into());
        }
    }
    let wheel = values.remove("--wheel").ok_or("--wheel required")?;
    let output = values.remove("--output").ok_or("--output required")?;
    let lock = if let Some(lock) = values.remove("--lock") {
        lock
    } else {
        std::env::current_dir()?
            .ancestors()
            .map(|p| p.join("uv.lock"))
            .find(|p| p.is_file())
            .ok_or("uv.lock not found; pass --lock")?
    };
    let result = openpilot_dev_tools::torque_stage::stage(&wheel, &output, &lock)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("stage torque numerics: {error}");
        std::process::exit(1);
    }
}
