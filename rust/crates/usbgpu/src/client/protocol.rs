use crate::Error;
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::{
    io::Read,
    process::ChildStdout,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn receive(
    stdout: &mut ChildStdout,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, Error> {
    let started = Instant::now();
    let mut line = Vec::new();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(Error::Contract("precompiled eGPU worker timed out"));
        }
        let mut descriptors = [PollFd::new(&*stdout, PollFlags::IN)];
        let interval_duration = remaining.min(Duration::from_millis(100));
        let interval = Timespec {
            tv_sec: i64::try_from(interval_duration.as_secs())
                .map_err(|_| Error::Contract("worker timeout overflow"))?,
            tv_nsec: i64::from(interval_duration.subsec_nanos()),
        };
        match poll(&mut descriptors, Some(&interval)) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
        let mut byte = [0];
        if stdout.read(&mut byte)? == 0 {
            return Err(Error::Contract("precompiled eGPU worker exited"));
        }
        line.push(byte[0]);
        if line.len() > 128 << 10 {
            return Err(Error::Contract("worker response size limit"));
        }
        if byte[0] == b'\n' {
            break;
        }
    }
    if let Some(error) = line.strip_prefix(b"ERROR ") {
        return Err(Error::Protocol(serde_json::from_slice::<String>(error)?));
    }
    Ok(line)
}
