//! Mechanical cancellation peer, never an original/native policy oracle.
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
fn idle_fixture(mode: &str, ready: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if ![
        "sleep",
        "zero-ppoll",
        "fd-ppoll",
        "pipe-read",
        "futex",
        "exited",
    ]
    .contains(&mode)
    {
        return Err("unknown idle fixture mode".into());
    }
    if fs::read_dir("/proc/self/task")?.count() != 1 {
        return Err("idle fixture requires one actual kernel thread".into());
    }
    let (reader, _writer) = rustix::pipe::pipe()?;
    let word = std::sync::atomic::AtomicU32::new(0);
    fs::write(ready, b"ready")?;
    let timeout = rustix::event::Timespec {
        tv_sec: 3,
        tv_nsec: 0,
    };
    match mode {
        "sleep" => std::thread::sleep(Duration::from_secs(3)),
        "zero-ppoll" => {
            rustix::event::poll(&mut [], Some(&timeout))?;
        }
        "fd-ppoll" => {
            rustix::event::poll(
                &mut [rustix::event::PollFd::new(
                    &reader,
                    rustix::event::PollFlags::IN,
                )],
                Some(&timeout),
            )?;
        }
        "pipe-read" => {
            rustix::io::read(&reader, &mut [0_u8])?;
        }
        "futex" => {
            rustix::thread::futex::wait(&word, rustix::thread::futex::Flags::PRIVATE, 0, None)?;
        }
        "exited" => {}
        _ => unreachable!(),
    }
    Ok(())
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "--idle-observer") {
        if args.len() != 3 {
            return Err("idle fixture requires mode and ready path".into());
        }
        return idle_fixture(
            args[1].to_str().ok_or("idle mode UTF-8 required")?,
            Path::new(&args[2]),
        );
    }
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
