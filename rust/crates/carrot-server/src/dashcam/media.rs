use super::{paths::Paths, Failure, Service};
use crate::{config::Config, Error};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct Media {
    pub(super) paths: Paths,
    program: PathBuf,
}
impl Media {
    pub fn original(service: &Service, config: &Config) -> Arc<Self> {
        Self::for_test(
            service.root.clone(),
            config
                .state
                .parent()
                .unwrap_or(&config.state)
                .join("cache/dashcam"),
            "ffmpeg".into(),
        )
    }
    pub fn for_test(root: PathBuf, cache: PathBuf, program: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            paths: Paths::new(root, cache),
            program,
        })
    }
    pub(super) fn run(
        &self,
        args: &[OsString],
        timeout: u64,
    ) -> Result<std::process::ExitStatus, Failure> {
        crate::screenrecord::ffmpeg::run(&self.program, args, Duration::from_secs(timeout))
            .map(|result| result.status)
            .map_err(|error| match error {
                crate::screenrecord::Failure::Http { status, message } => {
                    Failure::Http { status, message }
                }
                crate::screenrecord::Failure::Internal => {
                    Error::Source("dashcam media generation failed".into()).into()
                }
            })
    }
}
pub(super) fn positive(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}
pub(super) fn remove(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}
