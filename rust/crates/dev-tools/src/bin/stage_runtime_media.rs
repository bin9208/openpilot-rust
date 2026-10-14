use std::{collections::BTreeMap, path::PathBuf};
fn run() -> openpilot_dev_tools::seal::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid option")?;
        match key {
            "--help" | "-h" => {
                println!("stage-runtime-media --root ROOT --providers DIRECTORY --ffmpeg-report REPORT --graphics-report REPORT");
                return Ok(());
            }
            "--root" | "--providers" | "--ffmpeg-report" | "--graphics-report" => {
                let value = args.next().ok_or("missing option value")?;
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
    let result = openpilot_dev_tools::media_stage::stage(
        &values.remove("--root").ok_or("--root required")?,
        &values.remove("--providers").ok_or("--providers required")?,
        &values
            .remove("--ffmpeg-report")
            .ok_or("--ffmpeg-report required")?,
        &values
            .remove("--graphics-report")
            .ok_or("--graphics-report required")?,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("media staging: {error}");
        std::process::exit(1);
    }
}
