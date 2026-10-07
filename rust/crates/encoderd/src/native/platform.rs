#![allow(unsafe_code)]
use crate::{config::Mode, profile::Recording, Error};
use openpilot_logging::{native::Logger, record::Level, site::Site};
use openpilot_params::Params;
use std::{ffi::CString, sync::OnceLock};

static LOGGER: OnceLock<Logger> = OnceLock::new();
pub fn initialize() {
    if let Ok(logger) = Logger::for_runtime(
        env!("ENCODER_VERSION"),
        if cfg!(feature = "visionipc-ion") {
            "tici"
        } else {
            "pc"
        },
    ) {
        let _ = LOGGER.set(logger);
    }
}
pub fn emit(site: Site, level: Level, text: String) {
    if let Some(logger) = LOGGER.get() {
        let _ = logger.emit(site, level, text);
    }
}
pub fn close() {
    if let Some(logger) = LOGGER.get() {
        let _ = logger.close();
    }
}
pub fn schedule(mode: Mode) -> Result<(), Error> {
    let policy = mode.schedule(!cfg!(feature = "visionipc-ion"));
    if let Some(priority) = policy.priority {
        let parameter = libc::sched_param {
            sched_priority: priority,
        };
        // SAFETY: the initialized scheduling parameter is borrowed only by this syscall.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &parameter) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    if let Some(core) = policy.core {
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(core);
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}
pub fn name(name: &str) -> Result<(), Error> {
    let truncated = name.as_bytes().get(..15).unwrap_or(name.as_bytes());
    let name =
        CString::new(truncated).map_err(|_| Error::Contract("encoder thread name contains NUL"))?;
    rustix::thread::set_name(&name).map_err(std::io::Error::from)?;
    Ok(())
}
pub fn atoi(bytes: &[u8]) -> i32 {
    let bytes = bytes.split(|&byte| byte == 0).next().unwrap_or_default();
    let text = CString::new(bytes).expect("prefix excludes NUL");
    // SAFETY: a live NUL-terminated string is passed to the source's C parser.
    unsafe { libc::atoi(text.as_ptr()) }
}
pub fn stream_bitrate() -> i32 {
    use std::os::unix::ffi::OsStrExt;
    std::env::var_os("STREAM_BITRATE").map_or(600_000, |value| atoi(value.as_bytes()))
}
pub fn debug_encoder() -> i32 {
    use std::os::unix::ffi::OsStrExt;
    std::env::var_os("DEBUG_ENCODER").map_or(0, |value| atoi(value.as_bytes()))
}
pub fn segment_length() -> Result<i32, Error> {
    use std::os::unix::ffi::OsStrExt;
    if std::env::var_os("LOGGERD_TEST").is_none() {
        return Ok(60);
    }
    let value = std::env::var_os("LOGGERD_SEGMENT_LENGTH").ok_or(Error::Contract(
        "inherited LOGGERD_TEST atoi(null) boundary",
    ))?;
    Ok(atoi(value.as_bytes()))
}
pub fn recording() -> Result<Recording, Error> {
    let read = |key| -> Result<i32, Error> {
        let value = Params::for_runtime()?.get(key)?.unwrap_or_default();
        if value.is_empty() {
            return Ok(0);
        }
        let mut bytes = value.as_slice();
        while bytes.first().is_some_and(u8::is_ascii_whitespace) {
            bytes = &bytes[1..];
        }
        let mut length = usize::from(
            bytes
                .first()
                .is_some_and(|byte| matches!(byte, b'+' | b'-')),
        );
        let start = length;
        while bytes.get(length).is_some_and(u8::is_ascii_digit) {
            length += 1;
        }
        if length == start {
            return Err(Error::Contract("source Params std::stoi invalid argument"));
        }
        let text = std::str::from_utf8(&bytes[..length])
            .map_err(|_| Error::Contract("source Params integer encoding"))?;
        text.parse()
            .map_err(|_| Error::Contract("source Params std::stoi out of range"))
    };
    Ok(Recording {
        road: read("RecordRoadCam")? > 0,
        wide: read("RecordRoadCam")? > 1,
        front: Params::for_runtime()?.get_bool("RecordFront")?,
        audio: Params::for_runtime()?.get_bool("RecordAudio")?,
    })
}
