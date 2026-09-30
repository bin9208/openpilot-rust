use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{ffi::OsStringExt, net::UnixStream},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    ExitBefore,
    ExitConnected,
    SignalConnected,
    InvalidMarker,
    CorruptDescriptor,
    DelayedLaunch,
}

#[derive(Deserialize)]
struct Handshake {
    handshake: Vec<u8>,
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let control =
        PathBuf::from(std::env::var_os("CHECKOUT_GIT_FIXTURE").ok_or("missing fixture directory")?);
    let descriptor = PathBuf::from(std::env::args_os().nth(1).ok_or("missing descriptor")?);
    let mode: Mode = serde_json::from_slice(&fs::read(control.join("helper.json"))?)?;
    let connected = matches!(
        mode,
        Mode::ExitConnected | Mode::SignalConnected | Mode::InvalidMarker
    );
    let stream = if connected {
        let request: Handshake = serde_json::from_slice(&fs::read(&descriptor)?)?;
        Some(UnixStream::connect(PathBuf::from(
            std::ffi::OsString::from_vec(request.handshake),
        ))?)
    } else {
        None
    };
    let record = serde_json::json!({"mode": mode, "connected": connected, "pid": std::process::id(),
        "cwd": std::env::current_dir()?, "argv": std::env::args_os().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>()});
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(control.join("helper-calls.jsonl"))?
        .write_all(format!("{record}\n").as_bytes())?;
    match mode {
        Mode::ExitBefore | Mode::ExitConnected => Ok(7),
        Mode::SignalConnected => {
            rustix::process::kill_process(
                rustix::process::getpid(),
                rustix::process::Signal::KILL,
            )?;
            Ok(99)
        }
        Mode::InvalidMarker => {
            stream
                .ok_or("missing handshake stream")?
                .write_all(&[9; 5])?;
            std::thread::sleep(Duration::from_secs(30));
            Ok(99)
        }
        Mode::CorruptDescriptor => {
            fs::write(&descriptor, b"{")?;
            openpilot_process_supervision::run_child(File::open(descriptor)?)?;
            Ok(0)
        }
        Mode::DelayedLaunch => {
            std::thread::sleep(Duration::from_millis(700));
            openpilot_process_supervision::run_child(File::open(descriptor)?)?;
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("handshake fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
