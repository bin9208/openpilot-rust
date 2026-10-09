use crate::Error;
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    os::fd::{AsRawFd, BorrowedFd},
    sync::{atomic::AtomicBool, Arc},
};

fn close_extra_fds(error_pipe: i32, lock: Option<BorrowedFd<'_>>) -> Result<(), Error> {
    let descriptors = fs::read_dir("/proc/self/fd")?
        .map(|entry| {
            let entry = entry?;
            entry
                .file_name()
                .to_string_lossy()
                .parse::<i32>()
                .map_err(std::io::Error::other)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for descriptor in descriptors {
        if descriptor > 2
            && descriptor != error_pipe
            && lock.map(|fd| fd.as_raw_fd()) != Some(descriptor)
        {
            match nix::unistd::close(descriptor) {
                Ok(()) | Err(nix::errno::Errno::EBADF) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
    }
    Ok(())
}

pub(crate) fn prepare(error_pipe: i32) -> Result<(), Error> {
    prepare_with_lock(error_pipe, None)
}

pub(crate) fn prepare_with_lock(
    error_pipe: i32,
    lock: Option<BorrowedFd<'_>>,
) -> Result<(), Error> {
    close_extra_fds(error_pipe, lock)?;
    // exec resets caught handlers to default, matching Popen's restore_signals.
    for signal in [signal_hook::consts::SIGPIPE, signal_hook::consts::SIGXFSZ] {
        signal_hook::flag::register(signal, Arc::new(AtomicBool::new(false)))?;
    }
    Ok(())
}

fn execute(arguments: &[OsString], error_pipe: i32) -> Result<(), Error> {
    {
        let output = OpenOptions::new().write(true).open("/dev/null")?;
        rustix::stdio::dup2_stdout(&output).map_err(std::io::Error::from)?;
    }
    prepare(error_pipe)?;
    crate::exec::execute(arguments, &crate::exec::environment(None)?)
}

pub(crate) fn run(arguments: &[OsString]) -> Result<(), Error> {
    let descriptor =
        rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 3).map_err(std::io::Error::from)?;
    let mut error_pipe = File::from(descriptor);
    let error = match execute(arguments, error_pipe.as_raw_fd()) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let errno = match &error {
        Error::Io(error) => error.raw_os_error().unwrap_or(22),
        _ => 22,
    };
    error_pipe.write_all(&errno.to_ne_bytes())?;
    Err(error)
}
