//! Mechanical cancellation peer, never an original/native policy oracle.
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let records = PathBuf::from(
        std::env::var_os("LOGGERD_LIFECYCLE_RECORDS").ok_or("fixture records absent")?,
    );
    fs::create_dir_all(&records)?;
    let role = match std::env::args().nth(1).as_deref() {
        Some("--escaped") => "escaped",
        Some("--leaf") => "leaf",
        _ => "logger",
    };
    if role == "escaped" {
        rustix::process::setsid()?;
    }
    let _ignore = signal_hook::flag::register(
        signal_hook::consts::SIGTERM,
        Arc::new(AtomicBool::new(false)),
    )?;
    fs::write(
        records.join(format!("{role}.pending")),
        serde_json::to_vec(
            &json!({"pid":std::process::id(),"session":rustix::process::getsid(None)?.as_raw_pid(),"group":rustix::process::getpgrp().as_raw_pid()}),
        )?,
    )?;
    fs::rename(
        records.join(format!("{role}.pending")),
        records.join(format!("{role}.json")),
    )?;
    let child = if role != "leaf" {
        Some(
            Command::new(std::env::current_exe()?)
                .arg(if role == "escaped" {
                    "--leaf"
                } else {
                    "--escaped"
                })
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?,
        )
    } else {
        None
    };
    if role == "logger" {
        let prefix = std::env::var("OPENPILOT_PREFIX")?;
        let params = PathBuf::from(std::env::var_os("PARAMS_ROOT").ok_or("fixture params absent")?)
            .join(&prefix);
        fs::write(params.join("CurrentRoute"), b"00000000--fixture")?;
        fs::write(
            records.join("paths.pending"),
            serde_json::to_vec(
                &json!({"namespace":format!("/dev/shm/msgq_{prefix}"),"socket":format!("/tmp/logmessage{prefix}")}),
            )?,
        )?;
    }
    if role == "logger" {
        fs::rename(records.join("paths.pending"), records.join("paths.json"))?;
    }
    if role == "logger" && std::env::var("LOGGERD_LIFECYCLE_MODE").as_deref() == Ok("error") {
        let until = std::time::Instant::now() + Duration::from_secs(15);
        while !records.join("fail-now").exists() {
            if std::time::Instant::now() >= until {
                return Err("fixture error release timed out".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        return Err("intentional actual logger provider failure".into());
    }
    std::thread::sleep(Duration::from_secs(40));
    if let Some(mut child) = child {
        let _ = child.wait();
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("loggerd-lifecycle-fixture: {e}");
        std::process::exit(1);
    }
}
