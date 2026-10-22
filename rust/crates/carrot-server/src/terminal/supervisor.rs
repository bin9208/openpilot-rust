//! Terminal-only subreaper. Never enable adoption in the web server process.
#![forbid(unsafe_code)]
#[path = "supervisor_session.rs"]
mod session;
#[path = "supervisor_status.rs"]
mod status;

use openpilot_process_supervision::CapturedCommand;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
};

fn observe(pid: Pid) -> io::Result<Option<rustix::process::WaitIdStatus>> {
    loop {
        match rustix::process::waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Err(rustix::io::Errno::INTR) => continue,
            result => return result.map_err(Into::into),
        }
    }
}

fn run(
    control: &mut UnixStream,
    args: &[std::ffi::OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    rustix::process::set_child_subreaper(Some(Pid::INIT))?;
    let slave = File::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let child = CapturedCommand {
        launcher: args[0].clone().into(),
        cwd: std::env::current_dir()?,
        argv: vec![args[2].clone(), "-lc".into(), args[3].clone()],
    }
    .spawn_session_pty_with_env(slave, &[])
    .map_err(|error| match error {
        openpilot_process_supervision::Error::Io(error) => error,
        error => io::Error::other(error),
    })?;
    let pid = Pid::from_raw(i32::try_from(child.process.id())?).ok_or("PTY PID out of range")?;
    let mut owned = session::Owned {
        child,
        pid,
        closed: false,
        cleaned: false,
        deadline: None,
    };
    // The helper must not hold the slave open after the shell closes its copy.
    let null = File::options().read(true).write(true).open("/dev/null")?;
    rustix::stdio::dup2_stdin(&null)?;
    rustix::stdio::dup2_stdout(&null)?;
    rustix::stdio::dup2_stderr(&null)?;
    writeln!(control, "PID {}", pid.as_raw_nonzero())?;
    control.set_read_timeout(Some(std::time::Duration::from_millis(50)))?;
    loop {
        // Natural shell exit can otherwise be hidden forever by a background
        // job holding a PTY slave. Cleanup creates EOF for the actual owner loop.
        if !owned.cleaned && observe(pid)?.is_some() {
            owned.terminate()?;
        }
        let mut request = [0];
        match control.read(&mut request) {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
            Ok(0) => request[0] = b'X',
            Ok(_) => {}
        }
        if request[0] == b'X' {
            owned.close()?;
            writeln!(control, "CLOSED")?;
            return Ok(());
        }
        if request[0] == b'?' {
            match observe(pid)? {
                None => writeln!(control, "RUNNING")?,
                Some(status) => writeln!(control, "EXIT {}", status::exit_status(status))?,
            }
        } else if let Some(signal) = Signal::from_named_raw(i32::from(request[0])) {
            session::signal(pid, signal)?;
            writeln!(control, "OK")?;
        } else {
            return Err("invalid terminal supervisor request".into());
        }
    }
}

pub(super) fn entry(args: &[std::ffi::OsString]) -> std::process::ExitCode {
    if args.len() != 4 {
        return std::process::ExitCode::FAILURE;
    }
    let Ok(mut control) = UnixStream::connect(&args[1]) else {
        return std::process::ExitCode::FAILURE;
    };
    match run(&mut control, args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if let Some(errno) = error
                .downcast_ref::<io::Error>()
                .and_then(io::Error::raw_os_error)
            {
                let _ = writeln!(control, "ERRNO {errno}");
            } else {
                let _ = writeln!(control, "ERROR {error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
