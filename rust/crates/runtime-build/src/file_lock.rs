//! Release an operation's flock even if a concurrent fork retained its file description.
use crate::Result;
use std::fs::File;

pub(crate) struct FileLock(File);

impl FileLock {
    pub(crate) fn exclusive(file: File) -> Result<Self> {
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self(file))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Closing only this descriptor leaves the lock held by any fork/dup
        // copy. The protected operation is over on both success and error.
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    #[test]
    fn completed_or_failed_operation_releases_lock_with_a_live_duplicate() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        let open = || {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .unwrap()
        };
        for fail in [false, true] {
            let file = open();
            // dup shares the same open file description, as an inherited fork
            // descriptor does, without relying on a scheduler race in the test.
            let inherited = file.try_clone().unwrap();
            let operation = || -> Result<()> {
                let _lock = FileLock::exclusive(file)?;
                assert!(FileLock::exclusive(open()).is_err());
                if fail {
                    return Err("operation failed".into());
                }
                Ok(())
            };
            assert_eq!(operation().is_err(), fail);
            let next = FileLock::exclusive(open()).unwrap();
            assert!(inherited.metadata().unwrap().is_file());
            drop(next);
            drop(inherited);
        }
    }
}
