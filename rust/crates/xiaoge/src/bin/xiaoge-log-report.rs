//! Offline report only; this executable never selects or runs an inference backend.
use openpilot_xiaoge::log_report::{report_jsonl, report_rlogs};
use std::{
    error::Error,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

const USAGE: &str = "usage: xiaoge-log-report [--output PATH] RLOG[.zst]...\n       xiaoge-log-report [--output PATH] --jsonl PATH";
fn run() -> Result<(), Box<dyn Error>> {
    let mut output = None;
    let mut jsonl = None;
    let mut paths = Vec::new();
    let mut arguments = std::env::args_os().skip(1);
    let mut positional = false;
    while let Some(argument) = arguments.next() {
        if positional {
            paths.push(PathBuf::from(argument));
            continue;
        }
        match argument.to_str() {
            Some("--") => positional = true,
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some("--output") => {
                if output.is_some() {
                    return Err("--output supplied twice".into());
                }
                output = Some(PathBuf::from(
                    arguments.next().ok_or("--output needs a path")?,
                ));
            }
            Some("--jsonl") => {
                if jsonl.is_some() {
                    return Err("--jsonl supplied twice".into());
                }
                jsonl = Some(PathBuf::from(
                    arguments.next().ok_or("--jsonl needs a path")?,
                ));
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option: {value}\n{USAGE}").into())
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }
    if let Some(target) = output.as_ref().and_then(|path| fs::canonicalize(path).ok()) {
        for source in paths.iter().chain(jsonl.iter()) {
            if fs::canonicalize(source)? == target {
                return Err("--output must not overwrite an input log".into());
            }
        }
    }
    let report = match jsonl {
        Some(path) if paths.is_empty() => report_jsonl(path)?,
        Some(_) => return Err("--jsonl cannot be mixed with rlog inputs".into()),
        None if paths.is_empty() => return Err(USAGE.into()),
        None => report_rlogs(&paths)?,
    };
    // Validate both passes before opening output, so malformed logs cannot yield a partial report.
    let mut writer: Box<dyn Write> = match output {
        Some(path) => Box::new(File::create(path)?),
        None => Box::new(io::stdout().lock()),
    };
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writeln!(writer)?;
    writer.flush()?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("xiaoge-log-report: {error}");
        std::process::exit(1);
    }
}
