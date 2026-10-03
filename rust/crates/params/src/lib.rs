use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{symlink, OpenOptionsExt},
    path::{Path, PathBuf},
};

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
        let params = Self { root, directory };
        let _lock = params.lock()?;
        if !params.directory.exists() {
            let temporary = tempfile::Builder::new()
                .prefix(".tmp_")
                .tempdir_in(&params.root)?;
            match symlink(temporary.path(), &params.directory) {
                Ok(()) => {
                    File::open(temporary.keep())?.sync_all()?;
                }
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && params.directory.is_dir() => {}
                Err(error) => return Err(error.into()),
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

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        match fs::read(self.path(key)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
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
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    pub fn remove(&self, key: &str) -> Result<(), Error> {
        let path = self.path(key)?;
        let _lock = self.lock()?;
        fs::remove_file(path)?;
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
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
