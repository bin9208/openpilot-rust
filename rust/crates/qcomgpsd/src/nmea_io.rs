use crate::{nmea, runtime::pause, Error};
use rustix::{
    event::{poll, PollFd, PollFlags},
    fs::OFlags,
};
use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn line(bytes: &[u8]) -> Result<(), Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Protocol("NMEA invalid UTF-8"))?;
    if std::env::var("DEBUG").is_ok_and(|value| value.parse::<i64>().is_ok_and(|value| value != 0))
    {
        println!("{}", text.trim());
    }
    let text = text.trim();
    if text.starts_with('$') && !nmea::checksum_delimiter(text) {
        let reason = if text.chars().skip(1).any(|character| character == '*') {
            "NMEA string does not have checksum delimiter in correct location:"
        } else {
            "NMEA string does not have checksum delimiter:"
        };
        println!("ERROR: {reason} {text}");
        return Ok(());
    }
    if let Some(message) = nmea::parse(text)? {
        println!("{message}");
    }
    Ok(())
}
fn session(device: &Path, stop: &AtomicBool) -> Result<(), Error> {
    let flags =
        i32::try_from(OFlags::NONBLOCK.bits()).map_err(|_| Error::Protocol("NMEA open flags"))?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(device)?;
    let mut pending = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let mut descriptors = [PollFd::new(&file, PollFlags::IN)];
        let timeout = rustix::time::Timespec::try_from(Duration::from_millis(50))
            .map_err(|_| Error::Protocol("NMEA poll timeout"))?;
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => continue,
            Ok(_) => (),
            Err(error) => return Err(error.into()),
        }
        let mut chunk = [0; 8192];
        match file.read(&mut chunk) {
            Ok(0) => {
                if !pending.is_empty() {
                    line(&pending)?;
                }
                return Ok(());
            }
            Ok(count) => pending.extend_from_slice(&chunk[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        }
        while let Some(end) = pending
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
        {
            line(&pending[..end])?;
            pending.drain(..=end);
        }
    }
    Ok(())
}
pub fn read(device: &Path, stop: &AtomicBool) -> Result<(), Error> {
    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = session(device, stop) {
            println!("{error}");
            match pause(Duration::from_secs(1), stop) {
                Ok(()) | Err(Error::Stopped) => (),
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}
