//! Bounded recovery input records. Logging failures never block recovery actions.
pub use openpilot_carrot_navi::json::Value;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub struct TouchLog {
    path: PathBuf,
    max_bytes: u64,
    boot_id: String,
    lock: Mutex<()>,
    cross_process: bool,
}
impl TouchLog {
    pub fn new(path: PathBuf, max_bytes: u64, boot_id: String) -> Self {
        Self {
            path,
            max_bytes,
            boot_id,
            lock: Mutex::new(()),
            cross_process: false,
        }
    }
    pub fn for_path(path: PathBuf) -> Self {
        let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .unwrap_or_default()
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
            .to_owned();
        let mut logger = Self::new(path, 262144, boot_id);
        logger.cross_process = true;
        logger
    }
    fn process_lock(&self) -> std::io::Result<Option<File>> {
        if !self.cross_process {
            return Ok(None);
        }
        let mut path = self.path.as_os_str().to_os_string();
        path.push(".lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(file)),
                Err(TryLockError::Error(error)) => return Err(error),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(std::io::Error::other("recovery diagnostic lock busy"))
                }
            }
        }
    }
    pub fn record(&self, event: &str, sync: bool, fields: &[(&str, Value)]) -> bool {
        self.record_with(event, sync, fields, || {
            let wall = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(v) => v.as_secs_f64(),
                Err(v) => -v.duration().as_secs_f64(),
            };
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            (wall, now.tv_sec as f64 + now.tv_nsec as f64 / 1e9)
        })
    }
    pub fn record_with(
        &self,
        event: &str,
        sync: bool,
        fields: &[(&str, Value)],
        clock: impl FnOnce() -> (f64, f64),
    ) -> bool {
        let Ok(_lock) = self.lock.lock() else {
            return false;
        };
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(self.path.parent().unwrap_or(Path::new(".")))?;
            // The source's thread mutex becomes a bounded interprocess lock
            // because the native action and display are isolated processes.
            let _process_lock = self.process_lock()?;
            if self.path.exists() && fs::metadata(&self.path)?.len() >= self.max_bytes {
                let mut archive = self.path.as_os_str().to_os_string();
                archive.push(".1");
                fs::rename(&self.path, PathBuf::from(archive))?;
            }
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            let (wall, mono) = clock();
            let mut object = vec![
                ("event", Value::text(event)),
                ("time", Value::Float(wall)),
                ("monotonic", Value::Float(mono)),
                ("boot_id", Value::text(&self.boot_id)),
            ];
            for (key, value) in fields {
                if object.iter().any(|(existing, _)| existing == key) {
                    return Err("duplicate recovery log field".into());
                }
                object.push((key, value.clone()));
            }
            let value = Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
                    .collect(),
            );
            file.write_all(format!("{}\n", value.encode()?).as_bytes())?;
            if sync {
                file.flush()?;
                file.sync_all()?;
            }
            Ok(())
        };
        write().is_ok()
    }
}
#[cfg(test)]
mod tests;
