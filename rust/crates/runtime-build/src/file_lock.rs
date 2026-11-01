//! Release an operation's flock even if a concurrent fork retained its file description.
use crate::Result;
use std::fs::File;

pub struct FileLock(File);

impl FileLock {
    pub fn exclusive(file: File) -> Result<Self> {
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
    fn closing_only_the_original_descriptor_keeps_the_duplicate_locked() {
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
        let original = open();
        rustix::fs::flock(
            &original,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        )
        .unwrap();
        let inherited = original.try_clone().unwrap();
        drop(original);
        assert_eq!(
            rustix::fs::flock(open(), rustix::fs::FlockOperation::NonBlockingLockExclusive)
                .unwrap_err(),
            rustix::io::Errno::WOULDBLOCK
        );
        rustix::fs::flock(&inherited, rustix::fs::FlockOperation::Unlock).unwrap();
        let _next = FileLock::exclusive(open()).unwrap();
    }

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
