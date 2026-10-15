//! Capture manager diagnostics without waiting for descendants to close stdout.
use crate::Error;
use std::{
    fs::{self, File},
    io::{self, BufReader, Read},
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Source {
    file: File,
    stopped: Arc<AtomicBool>,
    drain_until: Option<Instant>,
}
impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.stopped.load(Ordering::Acquire) {
                let deadline = self
                    .drain_until
                    .get_or_insert_with(|| Instant::now() + Duration::from_millis(250));
                if Instant::now() >= *deadline {
                    return Ok(0);
                }
            }
            match self.file.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.stopped.load(Ordering::Acquire) {
                        return Ok(0);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                result => return result,
            }
        }
    }
}
pub(super) struct Capture {
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Capture {
    pub(super) fn start(command: &mut Command, path: &Path) -> Result<Self, Error> {
        let (read, write) =
            rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC).map_err(io::Error::from)?;
        let flags = rustix::fs::fcntl_getfl(&read).map_err(io::Error::from)?;
        rustix::fs::fcntl_setfl(&read, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(io::Error::from)?;
        let write = File::from(write);
        command.stdout(write.try_clone()?).stderr(write);
        let stopped = Arc::new(AtomicBool::new(false));
        let mut source = BufReader::new(Source {
            file: read.into(),
            stopped: stopped.clone(),
            drain_until: None,
        });
        let path = path.to_owned();
        fs::write(&path, [])?;
        let worker = thread::Builder::new()
            .name("startup_log".into())
            .spawn(move || {
                let started = Instant::now();
                openpilot_startup_recovery::logs::capture(
                    &mut source,
                    &mut io::stdout().lock(),
                    |bytes| fs::write(&path, bytes),
                    || started.elapsed().as_secs_f64(),
                )
            })?;
        Ok(Self {
            stopped,
            worker: Some(worker),
        })
    }
    fn finish_inner(&mut self) -> Result<(), Error> {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("startup log capture panicked"))??;
        }
        Ok(())
    }
    pub(super) fn finish(mut self) -> Result<(), Error> {
        self.finish_inner()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if let Err(error) = self.finish_inner() {
            eprintln!("startup log capture: {error}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};
    #[test]
    fn capture_drains_partial_line_without_waiting_for_inherited_writer() {
        let (read, write) = rustix::pipe::pipe().unwrap();
        rustix::fs::fcntl_setfl(&read, rustix::fs::OFlags::NONBLOCK).unwrap();
        let mut writer = File::from(write);
        writer
            .write_all(b"before\nError: startup failed without a newline")
            .unwrap();
        let stopped = Arc::new(AtomicBool::new(true));
        let mut source = BufReader::new(Source {
            file: read.into(),
            stopped,
            drain_until: None,
        });
        let mut forwarded = Vec::new();
        let mut stored = Vec::new();
        let started = Instant::now();
        openpilot_startup_recovery::logs::capture(
            &mut source,
            &mut forwarded,
            |bytes| {
                stored = bytes.to_vec();
                Ok(())
            },
            || 0.0,
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            forwarded,
            b"before\nError: startup failed without a newline"
        );
        assert_eq!(stored, forwarded);
        assert!(source.fill_buf().unwrap().is_empty());
        drop(writer);
    }
}
