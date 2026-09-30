//! Paths keeps the source's import-time hardware flag and per-call environment
//! reads. OsString preserves Unix environment bytes through filesystem use.
use crate::Error;
use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
};

pub const DEFAULT_DOWNLOAD_CACHE_ROOT: &str = "/tmp/comma_download_cache";
#[derive(Debug, Clone, Copy)]
pub struct Platform {
    pub tici: bool,
    pub agnos: bool,
    pub darwin: bool,
}
impl Platform {
    pub fn detect() -> Self {
        Self {
            tici: Path::new("/TICI").is_file(),
            agnos: Path::new("/AGNOS").is_file(),
            darwin: cfg!(target_os = "macos"),
        }
    }
    pub const fn pc(self) -> bool {
        !self.tici
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Paths {
    pub pc: bool,
    pub darwin: bool,
}
impl Default for Paths {
    fn default() -> Self {
        let platform = Platform::detect();
        Self {
            pc: platform.pc(),
            darwin: platform.darwin,
        }
    }
}
fn prefix() -> OsString {
    std::env::var_os("OPENPILOT_PREFIX").unwrap_or_default()
}
fn join(base: &OsStr, child: &OsStr) -> OsString {
    let mut result = base.as_bytes().to_vec();
    if !result.is_empty() && !result.ends_with(b"/") {
        result.push(b'/');
    }
    result.extend_from_slice(child.as_bytes());
    OsString::from_vec(result)
}
fn normalized(path: &OsStr) -> OsString {
    let bytes = path.as_bytes();
    let leading = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".as_slice()
    } else if bytes.starts_with(b"/") {
        b"/".as_slice()
    } else {
        b"".as_slice()
    };
    let components: Vec<_> = bytes
        .split(|&byte| byte == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
        .collect();
    let mut result = leading.to_vec();
    result.extend(components.join(&b'/'));
    if result.is_empty() {
        result.push(b'.');
    }
    OsString::from_vec(result)
}
impl Paths {
    pub fn comma_home(&self) -> Result<OsString, Error> {
        let home = match std::env::var_os("HOME") {
            Some(value) if value.is_empty() => OsString::from("/"),
            Some(value) => value,
            None => home::home_dir().ok_or(Error::Home)?.into_os_string(),
        };
        // expanduser("~") removes trailing separators before Path normalizes it.
        let end = home
            .as_bytes()
            .iter()
            .rposition(|&byte| byte != b'/')
            .map_or(0, |index| index + 1);
        let home = if end == 0 {
            OsString::from("/")
        } else {
            OsString::from_vec(home.as_bytes()[..end].to_vec())
        };
        let mut directory = OsString::from(".comma");
        directory.push(prefix());
        Ok(join(&normalized(&home), &directory))
    }
    pub fn log_root(&self) -> Result<OsString, Error> {
        if let Some(value) = std::env::var_os("LOG_ROOT").filter(|value| !value.is_empty()) {
            return Ok(value);
        }
        if self.pc {
            Ok(normalized(&join(
                &self.comma_home()?,
                OsStr::new("media/0/realdata"),
            )))
        } else {
            Ok("/data/media/0/realdata/".into())
        }
    }
    pub fn swaglog_root(&self) -> Result<OsString, Error> {
        if self.pc {
            Ok(join(&self.comma_home()?, OsStr::new("log")))
        } else {
            Ok("/data/log/".into())
        }
    }
    pub fn swaglog_ipc(&self) -> OsString {
        let mut result = OsString::from("ipc:///tmp/logmessage");
        result.push(prefix());
        result
    }
    pub fn download_cache_root(&self) -> OsString {
        let mut result = if let Some(cache) =
            std::env::var_os("COMMA_CACHE").filter(|value| !value.is_empty())
        {
            cache
        } else {
            let mut path = OsString::from(DEFAULT_DOWNLOAD_CACHE_ROOT);
            path.push(prefix());
            path
        };
        result.push("/");
        result
    }
    pub fn persist_root(&self) -> Result<OsString, Error> {
        if self.pc {
            Ok(join(&self.comma_home()?, OsStr::new("persist")))
        } else {
            Ok("/persist/".into())
        }
    }
    pub fn stats_root(&self) -> Result<OsString, Error> {
        if self.pc {
            Ok(normalized(&join(&self.comma_home()?, OsStr::new("stats"))))
        } else {
            Ok("/data/stats/".into())
        }
    }
    pub fn config_root(&self) -> Result<OsString, Error> {
        if self.pc {
            self.comma_home()
        } else {
            Ok("/tmp/.comma".into())
        }
    }
    pub fn shm_path(&self) -> &'static str {
        if self.pc && self.darwin {
            "/tmp"
        } else {
            "/dev/shm"
        }
    }
}
