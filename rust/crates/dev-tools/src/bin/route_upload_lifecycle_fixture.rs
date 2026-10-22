//! Mechanical failure peers for ownership tests; no runtime policy oracle.
use serde_json::json;
use std::{
    fs,
    os::unix::{net::UnixListener, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn record(role: &str) -> Result<()> {
    let root = PathBuf::from(
        std::env::var_os("ROUTE_UPLOAD_FIXTURE_RECORDS").ok_or("fixture records missing")?,
    );
    fs::create_dir_all(&root)?;
    fs::write(
        root.join(format!("{role}.json")),
        serde_json::to_vec(
            &json!({"pid":std::process::id(),"group":rustix::process::getpgrp().as_raw_pid(),"session":rustix::process::getsid(None)?.as_raw_pid()}),
        )?,
    )?;
    Ok(())
}
fn hold() {
    thread::sleep(Duration::from_secs(40));
}
fn role() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--escaped") {
        rustix::process::setsid()?;
        let _handler = signal_hook::flag::register(
            signal_hook::consts::SIGTERM,
            Arc::new(AtomicBool::new(false)),
        )?;
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--leaf")
            .spawn()?;
        record("escaped")?;
        hold();
        let _ = child.wait();
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--leaf") {
        let _handler = signal_hook::flag::register(
            signal_hook::consts::SIGTERM,
            Arc::new(AtomicBool::new(false)),
        )?;
        record("leaf")?;
        hold();
        return Ok(());
    }
    let mode = std::env::var("ROUTE_UPLOAD_LIFECYCLE_FIXTURE")?;
    if mode == "collector-actual-exit"
        && std::env::args_os().next().is_some_and(|arg| {
            Path::new(&arg).file_name() == Some(std::ffi::OsStr::new("openpilot-loggerd"))
        })
    {
        record("logger")?;
        hold();
        return Ok(());
    }
    if mode == "probe-escape"
        || mode == "probe-early-namespace"
        || mode == "probe-rebind"
        || mode == "probe-wait-before-bind"
    {
        let managed = PathBuf::from(std::env::args_os().nth(1).ok_or("managed output missing")?);
        let namespace = PathBuf::from(
            std::env::var_os("ROUTE_UPLOAD_QA_NAMESPACE").ok_or("owned namespace missing")?,
        );
        let prefix = namespace
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("msgq_"))
            .ok_or("invalid owned namespace")?;
        let records = PathBuf::from(
            std::env::var_os("ROUTE_UPLOAD_FIXTURE_RECORDS").ok_or("records missing")?,
        );
        record("probe")?;
        if mode == "probe-early-namespace" {
            fs::write(
                records.join("paths.json"),
                serde_json::to_vec(&json!({"namespace":namespace}))?,
            )?;
            hold();
            return Ok(());
        }
        fs::create_dir_all(managed.join("params").join(prefix))?;
        let ipc_root = PathBuf::from(
            std::env::var_os("ROUTE_UPLOAD_QA_IPC_ROOT").ok_or("QA IPC root missing")?,
        );
        if mode == "probe-wait-before-bind" {
            fs::write(records.join("before-bind"), b"ready")?;
            let deadline = Instant::now() + Duration::from_secs(10);
            while !records.join("continue-bind").exists() {
                if Instant::now() >= deadline {
                    return Err("test bind barrier deadline".into());
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        let socket = ipc_root.join("logmessage");
        let _log = UnixListener::bind(&socket)?;
        let stats_path = ipc_root.join("stats");
        let stats = UnixListener::bind(&stats_path)?;
        if mode == "probe-rebind" {
            fs::write(
                records.join("paths.json"),
                serde_json::to_vec(
                    &json!({"namespace":namespace,"socket":socket,"stats":stats_path,"ipc_root":ipc_root}),
                )?,
            )?;
            let journal = managed
                .parent()
                .ok_or("managed parent missing")?
                .join("ownership.jsonl");
            let deadline = Instant::now() + Duration::from_secs(5);
            while !fs::read_to_string(&journal).is_ok_and(|text| {
                text.lines().any(|line| {
                    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|row| {
                        row["path"].as_array().is_some_and(|bytes| {
                            bytes
                                .iter()
                                .filter_map(serde_json::Value::as_u64)
                                .map(|b| b as u8)
                                .collect::<Vec<_>>()
                                == stats_path.as_os_str().as_encoded_bytes()
                        })
                    })
                })
            }) {
                if Instant::now() >= deadline {
                    return Err("first stats identity not acknowledged".into());
                }
                thread::sleep(Duration::from_millis(5));
            }
            drop(stats);
            fs::remove_file(&stats_path)?;
            let _replacement = UnixListener::bind(&stats_path)?;
            fs::write(records.join("rebound"), b"ready")?;
            hold();
            return Ok(());
        }
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--escaped")
            .spawn()?;
        fs::write(
            records.join("paths.json"),
            serde_json::to_vec(
                &json!({"namespace":namespace,"socket":socket,"stats":stats_path,"ipc_root":ipc_root}),
            )?,
        )?;
        hold();
        let _ = child.wait();
    } else if mode == "collector-partial"
        || mode == "collector-partial-exit"
        || mode == "collector-actual-exit"
    {
        let prefix = std::env::var("OPENPILOT_PREFIX")?;
        let ipc_root = PathBuf::from(
            std::env::var_os("ROUTE_UPLOAD_QA_IPC_ROOT").ok_or("QA IPC root missing")?,
        );
        let socket = ipc_root.join("logmessage");
        if mode == "collector-actual-exit" {
            let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
            fs::create_dir(namespace.join("errorLogMessage"))?;
            record("collector")?;
            let records = PathBuf::from(
                std::env::var_os("ROUTE_UPLOAD_FIXTURE_RECORDS").ok_or("records missing")?,
            );
            fs::write(
                records.join("paths.json"),
                serde_json::to_vec(
                    &json!({"namespace":namespace,"socket":socket,"ipc_root":ipc_root}),
                )?,
            )?;
            let binary = PathBuf::from(
                std::env::var_os("ROUTE_UPLOAD_LIVE_BINARIES").ok_or("actual binaries missing")?,
            )
            .join("openpilot-logmessaged");
            let error = Command::new(binary)
                .args(std::env::args_os().skip(1))
                .exec();
            return Err(error.into());
        }
        let _listener = UnixListener::bind(&socket)?;
        record("collector")?;
        let records = PathBuf::from(
            std::env::var_os("ROUTE_UPLOAD_FIXTURE_RECORDS").ok_or("records missing")?,
        );
        fs::write(
            records.join("paths.json"),
            serde_json::to_vec(
                &json!({"namespace":format!("/dev/shm/msgq_{prefix}"),"socket":socket,"ipc_root":ipc_root}),
            )?,
        )?;
        if mode == "collector-partial-exit" {
            // No acknowledgement or synchronization: bind, publish diagnostics, exit.
            std::process::exit(7);
        }
        hold();
    } else if mode == "logger-sleep" {
        record("logger")?;
        hold();
    } else {
        return Err("unknown route ownership fixture role".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = role() {
        eprintln!("route-upload ownership fixture: {error}");
        std::process::exit(1);
    }
}
