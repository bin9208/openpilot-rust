//! The original C++ Params subset used by Sentry, with its Cython error boundary.
//! It is deliberately independent of the candidate Params implementation.
use super::{value as v, Error, Result};
use openpilot_log_format::JsonValue as J;
use std::{
    fs::{self, DirBuilder, File},
    io::Write,
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
};

pub(super) struct Params {
    cwd: PathBuf,
    root: PathBuf,
    prefix: PathBuf,
    namespace: PathBuf,
}
fn physical(cwd: &Path, path: &Path) -> PathBuf {
    cwd.join(path)
}
// Params passes its std::string paths to POSIX functions as c_str(). Keep that
// boundary, rather than treating an embedded NUL as a new source exception.
fn c_path(path: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = path.as_os_str().as_bytes();
    std::ffi::OsString::from_vec(
        bytes[..bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len())]
            .to_vec(),
    )
    .into()
}
impl Params {
    pub(super) fn new(cwd: &Path, path: &J) -> Result<Self> {
        // The binding calls str(path).encode(), which is strict UTF-8 even
        // though ordinary Python filesystem operations use surrogateescape.
        let text = v::text(v::string(path)).to_utf8().ok_or_else(|| {
            Error::new("UnicodeEncodeError", "Params path is not UTF-8 encodable")
        })?;
        let original = PathBuf::from(text);
        let root = physical(cwd, &c_path(&original));
        let prefix = original.join("fixture");
        let namespace = physical(cwd, &c_path(&prefix));
        let ensure = || -> std::io::Result<()> {
            if fs::metadata(&root).is_err() {
                DirBuilder::new()
                    .recursive(true)
                    .mode(0o775)
                    .create(&root)?;
            }
            if fs::metadata(&namespace).is_err() {
                let temporary = tempfile::Builder::new()
                    .prefix(".tmp_")
                    .rand_bytes(6)
                    .tempdir_in(&root)?
                    .keep();
                let link = temporary.with_extension("link");
                let target = original.join(temporary.file_name().expect("temporary basename"));
                std::os::unix::fs::symlink(target, &link)?;
                match fs::rename(&link, &namespace) {
                    Ok(()) => {}
                    Err(error) if error.raw_os_error() == Some(17) => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        };
        ensure().map_err(|error| Error::new("RuntimeError", error.to_string()))?;
        Ok(Self {
            cwd: cwd.into(),
            root,
            prefix,
            namespace,
        })
    }
    fn key_path(&self, key: &str) -> PathBuf {
        physical(&self.cwd, &c_path(&self.prefix.join(key)))
    }
    pub(super) fn get(&self, key: &str) -> Vec<u8> {
        // util::read_file returns an empty string for absent/unreadable input;
        // get_bool compares raw bytes and does not use STRING conversion.
        fs::read(self.key_path(key)).unwrap_or_default()
    }
    pub(super) fn put(&self, key: &str, bytes: &[u8]) {
        let put = || -> std::io::Result<()> {
            let mut temporary = tempfile::Builder::new()
                .prefix(".tmp_value_")
                .rand_bytes(6)
                .tempfile_in(&self.root)?;
            temporary.write_all(bytes)?;
            temporary.as_file().sync_all()?;
            let lock = rustix::fs::open(
                self.root.join(".lock"),
                rustix::fs::OFlags::CREATE | rustix::fs::OFlags::RDONLY,
                rustix::fs::Mode::from_raw_mode(0o775),
            );
            // The original FileLock logs a failed open/flock and still attempts
            // rename. The binding ignores Params::put's integer return value.
            let _lock = lock.ok().inspect(|file| {
                let _ = rustix::fs::flock(file, rustix::fs::FlockOperation::LockExclusive);
            });
            fs::rename(temporary.path(), self.key_path(key))?;
            File::open(&self.namespace)?.sync_all()
        };
        let _ = put();
    }
}
