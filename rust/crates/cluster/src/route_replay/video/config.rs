use crate::route_replay::Error;
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VideoConfig {
    pub ffmpeg: Option<PathBuf>,
    pub cwd: PathBuf,
}
fn executable(path: &Path) -> bool {
    path.is_file() && rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
}
fn which(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 || program.is_absolute() {
        return executable(program).then(|| program.to_path_buf());
    }
    let search = env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into());
    env::split_paths(&search)
        .map(|folder| folder.join(program))
        .find(|path| executable(path))
}
impl VideoConfig {
    /// Uses the source env override, then PATH, then an explicitly packaged provider.
    /// No Python interpreter is invoked for the imageio-FFmpeg fallback.
    /// # Errors
    /// Returns non-Unicode configuration instead of silently dropping the override.
    pub fn discover(bundled_ffmpeg: Option<&Path>, cwd: PathBuf) -> Result<Self, Error> {
        let value = match env::var("CLUSTER_ROUTE_FFMPEG") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => String::new(),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(Error::Invalid("CLUSTER_ROUTE_FFMPEG is not UTF-8".into()));
            }
        };
        let value = value.trim_matches(openpilot_startup_ui::text::whitespace);
        let override_path = if value.is_empty() {
            None
        } else {
            let raw = Path::new(value);
            let resolved = which(raw).unwrap_or_else(|| raw.to_path_buf());
            (resolved.exists() || which(&resolved).is_some()).then_some(resolved)
        };
        Ok(Self {
            ffmpeg: override_path
                .or_else(|| which(Path::new("ffmpeg")))
                .or_else(|| bundled_ffmpeg.map(Path::to_path_buf)),
            cwd,
        })
    }
}
