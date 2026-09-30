//! Log-space retention policy from system/loggerd/deleter.py.
mod names;
pub mod platform;
pub use names::sort_key as directory_sort_key;

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

pub const MIN_BYTES: u128 = 5 * 1024 * 1024 * 1024;
pub const MIN_PERCENT: f64 = 30.0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("filesystem block count is zero")]
    ZeroBlocks,
    #[error("{0}")]
    Arguments(&'static str),
}

pub struct Cycle {
    pub deleted: Option<OsString>,
    pub wait: Duration,
}

pub struct Deleter {
    root: PathBuf,
    preserve_cache: HashMap<OsString, bool>,
}

impl Deleter {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            preserve_cache: HashMap::new(),
        }
    }

    fn directories(&self) -> Vec<OsString> {
        if !self.root.is_dir() {
            return Vec::new();
        }
        let result = (|| -> io::Result<Vec<OsString>> {
            let mut names = Vec::new();
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    names.push(entry.file_name());
                }
            }
            names.sort_by_cached_key(|name| names::sort_key(name));
            Ok(names)
        })();
        result.unwrap_or_else(|error| {
            eprintln!("deleter: listdir_by_creation failed: {error}");
            Vec::new()
        })
    }

    fn has_preserve(&mut self, name: &OsString) -> Result<bool, Error> {
        if let Some(value) = self.preserve_cache.get(name) {
            return Ok(*value);
        }
        let mut buffer = vec![0_u8; 65536];
        let value =
            match rustix::fs::getxattr(self.root.join(name), "user.preserve", &mut buffer[..]) {
                Ok(length) => &buffer[..length] == b"1",
                Err(rustix::io::Errno::NODATA) => false,
                Err(error) => return Err(io::Error::from(error).into()),
            };
        self.preserve_cache.insert(name.clone(), value);
        Ok(value)
    }

    fn preserved_from(&mut self, directories: &[OsString]) -> Result<HashSet<OsString>, Error> {
        let mut preserved = HashSet::new();
        let mut seen = 0;
        for directory in directories.iter().rev() {
            if self.has_preserve(directory)? {
                if seen == 5 {
                    break;
                }
                seen += 1;
                preserved.extend(names::segment_and_prior(directory));
            }
        }
        Ok(preserved)
    }

    pub fn preserved(&mut self) -> Result<HashSet<OsString>, Error> {
        self.preserved_from(&self.directories())
    }

    pub fn cycle(&mut self, available_bytes: u128, available_percent: f64) -> Result<Cycle, Error> {
        if !(available_bytes < MIN_BYTES || available_percent < MIN_PERCENT) {
            return Ok(Cycle {
                deleted: None,
                wait: Duration::from_secs(30),
            });
        }
        let mut directories = self.directories();
        let preserved = self.preserved_from(&directories)?;
        directories
            .sort_by_key(|name| (name == "boot" || name == "crash", preserved.contains(name)));
        let mut deleted = None;
        for directory in directories {
            let path = self.root.join(&directory);
            let result = (|| -> io::Result<bool> {
                for entry in fs::read_dir(&path)? {
                    if entry?.file_name().as_encoded_bytes().ends_with(b".lock") {
                        return Ok(false);
                    }
                }
                eprintln!("deleter: deleting {}", path.display());
                if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                    return Err(io::Error::other(
                        "cannot remove a symbolic link as a directory",
                    ));
                }
                fs::remove_dir_all(&path)?;
                Ok(true)
            })();
            match result {
                Ok(true) => {
                    deleted = Some(directory);
                    break;
                }
                Ok(false) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => eprintln!("deleter: issue deleting {}: {error}", path.display()),
            }
        }
        Ok(Cycle {
            deleted,
            wait: Duration::from_millis(100),
        })
    }

    pub fn tick(&mut self) -> Result<Cycle, Error> {
        let bytes = platform::available_bytes(&self.root);
        let percent = platform::available_percent(&self.root)?;
        self.cycle(bytes, percent)
    }
}
