//! Native stand-in for the two Bluetooth permission commands used by the input fixtures.
use crate::{python_fs, seal::Result};
use rustix::process::{kill_process, Pid, Signal};
use std::{
    env,
    ffi::OsString,
    fs::OpenOptions,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    thread,
    time::Duration,
};

fn json_args(args: &[OsString]) -> Result<String> {
    let mut row = String::from("[");
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            row.push_str(", ");
        }
        row.push_str(&python_fs::json_string(arg.as_os_str())?);
    }
    row.push_str("]\n");
    Ok(row)
}
pub fn run(args: &[OsString], log: &Path, mode: &std::ffi::OsStr) -> Result<i32> {
    let row = json_args(args)?;
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)?
        .write_all(row.as_bytes())?;
    if mode == "timeout" {
        thread::sleep(Duration::from_secs(10));
    }
    if mode == "signal" {
        let pid = Pid::from_raw(i32::try_from(std::process::id())?)
            .ok_or("invalid Bluetooth sudo fixture process id")?;
        kill_process(pid, Signal::TERM)?;
    }
    if mode == "loud" {
        io::stdout().lock().write_all(&[b'o'; 1_048_576])?;
        io::stderr().lock().write_all(&[b'e'; 1_048_576])?;
    }
    let command = args
        .get(1)
        .ok_or("missing second Bluetooth sudo fixture argument")?;
    Ok(
        if mode
            .as_bytes()
            .strip_prefix(b"fail-")
            .is_some_and(|suffix| suffix == command.as_bytes())
        {
            7
        } else {
            0
        },
    )
}
pub fn cli() -> Result<i32> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let log = env::var_os("INPUT_FIXTURE_SUDO_LOG").ok_or("missing INPUT_FIXTURE_SUDO_LOG")?;
    let mode = env::var_os("INPUT_FIXTURE_SUDO_MODE").unwrap_or_else(|| "ok".into());
    run(&args, Path::new(&log), &mode)
}
