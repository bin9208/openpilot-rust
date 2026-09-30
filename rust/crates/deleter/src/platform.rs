use crate::{Error, MIN_BYTES, MIN_PERCENT};
use std::{
    env,
    path::{Path, PathBuf},
};

pub fn log_root() -> Result<PathBuf, Error> {
    if let Some(root) = env::var_os("LOG_ROOT").filter(|value| !value.is_empty()) {
        return Ok(root.into());
    }
    if Path::new("/TICI").is_file() {
        return Ok("/data/media/0/realdata".into());
    }
    let mut name = std::ffi::OsString::from(".comma");
    name.push(env::var_os("OPENPILOT_PREFIX").unwrap_or_default());
    Ok(home::home_dir()
        .ok_or(Error::Arguments("home directory unavailable"))?
        .join(name)
        .join("media/0/realdata"))
}

pub fn available_bytes(root: &Path) -> u128 {
    rustix::fs::statvfs(root).map_or(MIN_BYTES + 1, |stat| {
        u128::from(stat.f_bavail) * u128::from(stat.f_frsize)
    })
}

pub fn available_percent(root: &Path) -> Result<f64, Error> {
    match rustix::fs::statvfs(root) {
        Ok(stat) if stat.f_blocks == 0 => Err(Error::ZeroBlocks),
        Ok(stat) => Ok(100.0 * stat.f_bavail as f64 / stat.f_blocks as f64),
        Err(_) => Ok(MIN_PERCENT + 1.0),
    }
}
