use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{symlink, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// How often the namespace directory is re-checked for renamed-in values.
const DIRECTORY_CHECK: Duration = Duration::from_millis(10);
/// Values written in place do not touch the directory; re-read them this often.
const VALUE_LIFETIME: Duration = Duration::from_secs(1);

/// Handles on the same namespace share one cache, so a process opening several
/// Params handles checks the directory once per DIRECTORY_CHECK, not once per handle.
fn shared_cache(directory: &Path) -> Arc<Mutex<Cache>> {
    static CACHES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Cache>>>>> = OnceLock::new();
    let mut caches = CACHES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    Arc::clone(caches.entry(directory.to_owned()).or_default())
}

#[derive(Default)]
struct Cache {
    directory: Option<(i64, i64)>,
    checked: Option<Instant>,
    values: HashMap<String, (Instant, Option<Vec<u8>>)>,
}

mod read;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInfo {
    pub name: &'static str,
    pub flags: u32,
    pub kind: u8,
    pub default: Option<&'static str>,
}

include!(concat!(env!("OUT_DIR"), "/keys.rs"));

pub fn metadata(key: &str) -> Option<&'static KeyInfo> {
    KEYS.binary_search_by_key(&key, |info| info.name)
        .ok()
        .map(|index| &KEYS[index])
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid Params prefix")]
    InvalidPrefix,
    #[error("unknown Params key: {0}")]
    UnknownKey(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Clone)]
pub struct Params {
    root: PathBuf,
    directory: PathBuf,
    cache: Arc<Mutex<Cache>>,
}

impl Params {
    pub fn open(root: &Path, prefix: &str) -> Result<Self, Error> {
        Self::open_namespace(root, prefix, false)
    }

    pub fn for_runtime() -> Result<Self, Error> {
        let prefix = match std::env::var("OPENPILOT_PREFIX") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => return Err(Error::InvalidPrefix),
        };
        let root = match std::env::var_os("PARAMS_ROOT") {
            Some(path) => PathBuf::from(path),
            None if Path::new("/TICI").is_file() => PathBuf::from("/data/params"),
            None => {
                let mut path = std::env::var_os("HOME").unwrap_or_default();
                path.push("/.comma");
                path.push(prefix.as_deref().unwrap_or(""));
                path.push("/params");
                PathBuf::from(path)
            }
        };
        Self::open_namespace(&root, prefix.as_deref().unwrap_or("d"), true)
    }

    pub fn for_runtime_at(root: &Path) -> Result<Self, Error> {
        let prefix = match std::env::var("OPENPILOT_PREFIX") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => return Err(Error::InvalidPrefix),
        };
        Self::open_namespace(root, prefix.as_deref().unwrap_or("d"), true)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn open_namespace(root: &Path, prefix: &str, allow_empty: bool) -> Result<Self, Error> {
        if (!allow_empty && prefix.is_empty())
            || prefix.len() > 100
            || !prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        {
            return Err(Error::InvalidPrefix);
        }
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let directory = root.join(prefix);
        let params = Self {
            root,
            cache: shared_cache(&directory),
            directory,
        };
        let _lock = params.lock()?;
        if !params.directory.exists() {
            let temporary = tempfile::Builder::new()
                .prefix(".tmp_")
                .tempdir_in(&params.root)?;
            let mut link_path = temporary.path().as_os_str().to_os_string();
            link_path.push(".link");
            symlink(temporary.path(), &link_path)?;
            let link = tempfile::TempPath::try_from_path(link_path)?;
            match link.persist(&params.directory) {
                Ok(()) => {
                    File::open(temporary.keep())?.sync_all()?;
                }
                Err(failure)
                    if failure.error.kind() == io::ErrorKind::AlreadyExists
                        && params.directory.is_dir() => {}
                Err(failure) => return Err(failure.error.into()),
            }
            File::open(&params.root)?.sync_all()?;
        }
        if !params.directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "Params namespace is not a directory",
            )
            .into());
        }
        Ok(params)
    }

    fn lock(&self) -> io::Result<File> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o775)
            .open(self.root.join(".lock"))?;
        lock.lock()?;
        Ok(lock)
    }

    fn path(&self, key: &str) -> Result<PathBuf, Error> {
        metadata(key).ok_or_else(|| Error::UnknownKey(key.to_owned()))?;
        Ok(self.directory.join(key))
    }

    /// Values are re-read when the namespace changes. Every write renames into the
    /// directory, so a cached value is at most DIRECTORY_CHECK old after a write.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let path = self.path(key)?;
        let now = Instant::now();
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if cache
            .checked
            .is_none_or(|checked| now.duration_since(checked) >= DIRECTORY_CHECK)
        {
            let stamp = fs::metadata(&self.directory)
                .map(|meta| (meta.mtime(), meta.mtime_nsec()))
                .ok();
            if stamp.is_none() || stamp != cache.directory {
                cache.values.clear();
            }
            cache.directory = stamp;
            cache.checked = Some(now);
        }
        if let Some((read, value)) = cache.values.get(key) {
            if now.duration_since(*read) < VALUE_LIFETIME {
                return Ok(value.clone());
            }
        }
        let value = match File::open(path).and_then(|mut file| read::value(&mut file)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        cache.values.insert(key.to_owned(), (now, value.clone()));
        Ok(value)
    }

    fn forget(&self, key: Option<&str>) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match key {
            Some(key) => {
                cache.values.remove(key);
            }
            None => cache.values.clear(),
        }
    }

    pub fn get_bool(&self, key: &str) -> Result<bool, Error> {
        Ok(self.get(key)?.as_deref() == Some(b"1"))
    }

    pub fn put_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put(key, if value { b"1" } else { b"0" })
    }

    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        let path = self.path(key)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp_value_")
            .tempfile_in(&self.root)?;
        temporary.write_all(value)?;
        temporary.as_file().sync_all()?;
        let _lock = self.lock()?;
        temporary.persist(path).map_err(|error| error.error)?;
        self.forget(Some(key));
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    /// Persist only if the optional raw value still matches the caller's snapshot.
    /// The comparison and rename share the same lock as ordinary Params writes.
    pub fn put_if_unchanged(
        &self,
        key: &str,
        expected: Option<&[u8]>,
        value: &[u8],
    ) -> Result<bool, Error> {
        let path = self.path(key)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".tmp_value_")
            .tempfile_in(&self.root)?;
        temporary.write_all(value)?;
        temporary.as_file().sync_all()?;
        let _lock = self.lock()?;
        self.forget(Some(key));
        if self.get(key)?.as_deref() != expected {
            return Ok(false);
        }
        temporary.persist(path).map_err(|error| error.error)?;
        self.forget(Some(key));
        File::open(&self.directory)?.sync_all()?;
        Ok(true)
    }

    pub fn remove(&self, key: &str) -> Result<(), Error> {
        let path = self.path(key)?;
        let _lock = self.lock()?;
        fs::remove_file(path)?;
        self.forget(Some(key));
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    pub fn clear(&self, flags: u32) -> Result<(), Error> {
        let _lock = self.lock()?;
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let info = name.to_str().and_then(metadata);
            if info.is_none_or(|key| key.flags & flags != 0) {
                fs::remove_file(entry.path())?;
            }
        }
        self.forget(None);
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn sees_writes_from_other_handles() {
        let root = tempfile::tempdir().unwrap();
        let first = Params::open(root.path(), "d").unwrap();
        let second = Params::open(root.path(), "d").unwrap();
        assert_eq!(first.get("IsOnroad").unwrap(), None);
        second.put("IsOnroad", b"1").unwrap();
        std::thread::sleep(DIRECTORY_CHECK * 2);
        assert_eq!(first.get("IsOnroad").unwrap().as_deref(), Some(&b"1"[..]));
        second.remove("IsOnroad").unwrap();
        std::thread::sleep(DIRECTORY_CHECK * 2);
        assert_eq!(first.get("IsOnroad").unwrap(), None);
    }

    #[test]
    fn own_writes_are_immediate() {
        let root = tempfile::tempdir().unwrap();
        let params = Params::open(root.path(), "d").unwrap();
        params.put("IsOnroad", b"1").unwrap();
        assert_eq!(params.get("IsOnroad").unwrap().as_deref(), Some(&b"1"[..]));
        params.put("IsOnroad", b"0").unwrap();
        assert_eq!(params.get("IsOnroad").unwrap().as_deref(), Some(&b"0"[..]));
    }

    #[test]
    fn in_place_writes_expire() {
        let root = tempfile::tempdir().unwrap();
        let params = Params::open(root.path(), "d").unwrap();
        params.put("IsOnroad", b"1").unwrap();
        assert_eq!(params.get("IsOnroad").unwrap().as_deref(), Some(&b"1"[..]));
        fs::write(params.directory().join("IsOnroad"), b"0").unwrap();
        std::thread::sleep(VALUE_LIFETIME + DIRECTORY_CHECK);
        assert_eq!(params.get("IsOnroad").unwrap().as_deref(), Some(&b"0"[..]));
    }
}
