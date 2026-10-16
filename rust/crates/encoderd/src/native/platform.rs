#![allow(unsafe_code)]
use crate::{config::Mode, profile::Recording, Error};
use openpilot_logging::{native::Logger, record::Level, site::Site};
use openpilot_params::Params;
use std::{ffi::CString, sync::OnceLock};

pub const fn is_pc() -> bool {
    !openpilot_msgq::VISIONIPC_ION
}
static LOGGER: OnceLock<Logger> = OnceLock::new();
pub fn initialize() {
    if let Ok(logger) =
        Logger::for_runtime(env!("ENCODER_VERSION"), if is_pc() { "pc" } else { "tici" })
    {
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
    let policy = mode.schedule(is_pc());
    if let Some(priority) = policy.priority {
        // SAFETY: GNU/MUSL sched_param contains only C integers, integer timespecs,
        // and reserved storage; all-zero is valid for every field.
        let mut parameter: libc::sched_param = unsafe { std::mem::zeroed() };
        parameter.sched_priority = priority;
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

#[cfg(test)]
mod tests {
    use super::is_pc;
    use crate::config::{Codec, Mode, Quality, Scheduling};

    const INPUT_WIDTH: i32 = 3840;
    const STREAM_BITRATE: i32 = 600_000;

    fn expected_pc() -> bool {
        !openpilot_msgq::VISIONIPC_ION
    }

    #[test]
    fn main_codec_and_scheduling_follow_compiled_transport() {
        let pc = is_pc();
        let main = Mode::Main.schedule(pc);
        let stream = Mode::Stream.schedule(pc);
        let carrot_vision = Mode::CarrotVision.schedule(pc);
        let codec = Quality::Main
            .settings(INPUT_WIDTH, pc, STREAM_BITRATE)
            .codec;
        if expected_pc() {
            assert_eq!(codec, Codec::BigBoxLossless);
            assert_eq!(
                main,
                Scheduling {
                    priority: None,
                    core: None
                }
            );
            assert_eq!(
                stream,
                Scheduling {
                    priority: None,
                    core: None
                }
            );
            assert_eq!(
                carrot_vision,
                Scheduling {
                    priority: None,
                    core: None
                }
            );
        } else {
            assert_eq!(codec, Codec::FullHevc);
            assert_eq!(
                main,
                Scheduling {
                    priority: Some(52),
                    core: Some(3)
                }
            );
            assert_eq!(
                stream,
                Scheduling {
                    priority: Some(52),
                    core: Some(3)
                }
            );
            assert_eq!(
                carrot_vision,
                Scheduling {
                    priority: None,
                    core: Some(0)
                }
            );
        }
    }

    #[test]
    fn qcam_and_stream_stay_qcamera_h264() {
        let pc = is_pc();
        assert_eq!(
            Quality::Qcam
                .settings(INPUT_WIDTH, pc, STREAM_BITRATE)
                .codec,
            Codec::QcameraH264
        );
        assert_eq!(
            Quality::Stream
                .settings(INPUT_WIDTH, pc, STREAM_BITRATE)
                .codec,
            Codec::QcameraH264
        );
    }
}
