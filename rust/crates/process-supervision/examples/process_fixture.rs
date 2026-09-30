use serde::Serialize;
use signal_hook::iterator::Signals;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

#[derive(Serialize)]
struct Ready {
    pid: u32,
    group: i32,
    cwd: PathBuf,
    argv: Vec<String>,
    manager_daemon: Option<String>,
    inherited: Option<String>,
    stdin_target: PathBuf,
    stdout_target: PathBuf,
    stderr_target: PathBuf,
    inherited_fd_target: Option<PathBuf>,
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let root = PathBuf::from(args.get(1).ok_or("missing fixture directory")?);
    let mode = args.get(2).ok_or("missing fixture mode")?;
    let inherited_fd_target = std::env::var("PROCESS_FIXTURE_FD")
        .ok()
        .and_then(|fd| fs::read_link(format!("/proc/self/fd/{fd}")).ok());
    let mut signals = Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGUSR1,
    ])?;
    let ready = Ready {
        pid: std::process::id(),
        group: rustix::process::getpgrp().as_raw_pid(),
        cwd: std::env::current_dir()?,
        argv: args.clone(),
        manager_daemon: std::env::var("MANAGER_DAEMON").ok(),
        inherited: std::env::var("PROCESS_FIXTURE_INHERITED").ok(),
        stdin_target: fs::read_link("/proc/self/fd/0")?,
        stdout_target: fs::read_link("/proc/self/fd/1")?,
        stderr_target: fs::read_link("/proc/self/fd/2")?,
        inherited_fd_target,
    };
    fs::create_dir_all(&root)?;
    fs::write(root.join("ready.tmp"), serde_json::to_vec(&ready)?)?;
    fs::rename(root.join("ready.tmp"), root.join("ready.json"))?;
    let watchdog = Instant::now();
    loop {
        if let Ok(value) = fs::read_to_string(root.join("exit")) {
            fs::remove_file(root.join("exit"))?;
            return Ok(value.trim().parse()?);
        }
        for signal in signals.pending() {
            let mut log = OpenOptions::new()
                .append(true)
                .create(true)
                .open(root.join("signals.jsonl"))?;
            writeln!(log, "{signal}")?;
            if mode != "ignore" {
                if mode == "delay" {
                    thread::sleep(Duration::from_millis(200));
                }
                return Ok(0);
            }
        }
        if watchdog.elapsed() > Duration::from_secs(90) {
            return Ok(99);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("process fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
