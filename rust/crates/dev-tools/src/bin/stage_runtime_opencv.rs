use std::{collections::BTreeMap, path::PathBuf};
fn run() -> openpilot_dev_tools::seal::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(argument) = args.next() {
        let key = argument.to_str().ok_or("non-UTF8 option")?;
        match key {
            "--help" | "-h" => {
                println!("stage-runtime-opencv --root ROOT --provider PREFIX --evidence DIRECTORY");
                return Ok(());
            }
            "--root" | "--provider" | "--evidence" => {
                let value = args.next().ok_or("option value missing")?;
                if values
                    .insert(key.to_owned(), PathBuf::from(value))
                    .is_some()
                {
                    return Err("duplicate option".into());
                }
            }
            _ => return Err("unknown option".into()),
        }
    }
    let result = openpilot_dev_tools::opencv_stage::stage(
        &values.remove("--root").ok_or("--root required")?,
        &values.remove("--provider").ok_or("--provider required")?,
        &values.remove("--evidence").ok_or("--evidence required")?,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("OpenCV runtime staging: {error}");
        std::process::exit(1);
    }
}
