use super::pause;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File, OpenOptions, TryLockError},
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Paths {
    pub repository: PathBuf,
    pub data: PathBuf,
    pub temporary: PathBuf,
    pub system: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            repository: "/data/openpilot".into(),
            data: "/data".into(),
            temporary: "/tmp".into(),
            system: "/".into(),
        }
    }
}

impl Paths {
    pub(super) fn binary(&self, name: &str) -> PathBuf {
        self.repository.join("rust/bin").join(name)
    }
    pub(super) fn lock(&self) -> PathBuf {
        env::var_os("CARROT_REPO_LOCK_PATH").map_or_else(
            || self.temporary.join("carrot_repo_update.lock"),
            PathBuf::from,
        )
    }
    pub(super) fn environment(&self) -> Result<(), Error> {
        // A previous launcher environment must not disable this boot's selection.
        env::remove_var("CARROT_BIG_MODEL_STARTUP_FAILED");
        for (name, value) in [
            ("OMP_NUM_THREADS", "1"),
            ("MKL_NUM_THREADS", "1"),
            ("NUMEXPR_NUM_THREADS", "1"),
            ("OPENBLAS_NUM_THREADS", "1"),
            ("VECLIB_MAXIMUM_THREADS", "1"),
            ("QCOM_PRIORITY", "12"),
        ] {
            env::set_var(name, value);
        }
        for name in ["OPENPILOT_ROOT", "OPENPILOT_BASEDIR", "BASEDIR"] {
            env::set_var(name, &self.repository);
        }
        env::set_var("PARAMS_ROOT", self.data.join("params"));
        env::set_var("STAGING_ROOT", self.data.join("safe_staging"));
        env::set_var("CARROT_REPO_LOCK_PATH", self.lock());
        for (key, value) in [
            (
                "CARROT_WEB_PID_FILE",
                self.temporary.join("carrot_web_watchdog.pid"),
            ),
            (
                "CARROT_WEB_LOCK_FILE",
                self.temporary.join("carrot_web_watchdog.lock"),
            ),
            ("CARROT_DATA_DIR", self.data.join("carrot")),
        ] {
            if env::var_os(key).is_none() {
                env::set_var(key, value);
            }
        }
        if env::var_os("CARROT_WEB_EXTERNAL").is_none() {
            env::set_var("CARROT_WEB_EXTERNAL", "1");
        }
        let mut directories = vec![self.repository.join("rust/bin")];
        directories.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
        env::set_var(
            "PATH",
            env::join_paths(directories)
                .map_err(|error| Error::Configuration(error.to_string()))?,
        );
        let native = self.repository.join("rust/bin/lib");
        let mut libraries = vec![native.clone()];
        if let Some(previous) = env::var_os("LD_LIBRARY_PATH").filter(|value| !value.is_empty()) {
            libraries.extend(env::split_paths(&previous).filter(|path| path != &native));
        }
        env::set_var(
            "LD_LIBRARY_PATH",
            env::join_paths(libraries).map_err(|error| Error::Configuration(error.to_string()))?,
        );
        let mut count = env::var("GIT_CONFIG_COUNT")
            .unwrap_or_else(|_| "0".into())
            .parse::<u32>()
            .map_err(|_| Error::Configuration("invalid GIT_CONFIG_COUNT".into()))?;
        for (key, value) in [
            ("gc.auto", "0"),
            ("gc.autoDetach", "false"),
            ("maintenance.auto", "false"),
        ] {
            env::set_var(format!("GIT_CONFIG_KEY_{count}"), key);
            env::set_var(format!("GIT_CONFIG_VALUE_{count}"), value);
            count = count
                .checked_add(1)
                .ok_or_else(|| Error::Configuration("GIT_CONFIG_COUNT overflow".into()))?;
        }
        env::set_var("GIT_CONFIG_COUNT", count.to_string());
        Ok(())
    }
}

pub(super) fn acquire(paths: &Paths, stopped: &AtomicBool) -> Result<Option<File>, Error> {
    if let Some(socket) = env::var_os("CARROT_BOOT_LOCK_SOCKET") {
        env::remove_var("CARROT_BOOT_LOCK_SOCKET");
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let file = openpilot_process_supervision::receive_boot_lock(&mut stream)?;
        let actual = file.metadata()?;
        let expected = fs::metadata(paths.lock())?;
        if (actual.dev(), actual.ino()) != (expected.dev(), expected.ino()) {
            return Err(Error::Configuration(
                "inherited repository lock identity mismatch".into(),
            ));
        }
        return Ok(Some(file));
    }
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(paths.lock())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        if stopped.load(Ordering::Relaxed) || Instant::now() >= deadline {
            return Ok(None);
        }
        pause(Duration::from_millis(50), stopped);
    }
}
