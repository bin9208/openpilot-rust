//! Independent C++ Params/Cython subset used by the original Sentry handler.
use super::Error;
use std::{
    fs::{self, DirBuilder, File},
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};
pub(super) struct Params {
    root: PathBuf,
    namespace: PathBuf,
}
impl Params {
    pub(super) fn new(root: &Path, prefix: &str) -> Result<Self, Error> {
        let namespace = root.join(prefix);
        let ensure = || -> std::io::Result<()> {
            if fs::metadata(root).is_err() {
                DirBuilder::new().recursive(true).mode(0o775).create(root)?;
            }
            if fs::metadata(&namespace).is_err() {
                let temporary = tempfile::Builder::new()
                    .prefix(".tmp_")
                    .rand_bytes(6)
                    .tempdir_in(root)?
                    .keep();
                let link = temporary.with_extension("link");
                std::os::unix::fs::symlink(&temporary, &link)?;
                match fs::rename(&link, &namespace) {
                    Ok(()) => (),
                    Err(error) if error.raw_os_error() == Some(17) => (),
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        };
        ensure().map_err(|error| Error::new("RuntimeError", error.to_string()))?;
        Ok(Self {
            root: root.into(),
            namespace,
        })
    }
    pub(super) fn get(&self, key: &str) -> Vec<u8> {
        fs::read(self.namespace.join(key)).unwrap_or_default()
    }
    pub(super) fn put(&self, key: &str, bytes: &[u8]) {
        // The Cython wrapper ignores Params::put's integer error result.
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
            let _lock = lock.ok().inspect(|file| {
                let _ = rustix::fs::flock(file, rustix::fs::FlockOperation::LockExclusive);
            });
            fs::rename(temporary.path(), self.namespace.join(key))?;
            File::open(&self.namespace)?.sync_all()
        };
        let _ = put();
    }
}
