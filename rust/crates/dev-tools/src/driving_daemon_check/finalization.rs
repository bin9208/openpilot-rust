//! Owned report rollback remains armed until final verification and publication succeed.
use crate::seal::Result;
use serde_json::Value;
use std::{cell::Cell, fs::File, io::Write, os::unix::fs::MetadataExt};
pub(super) struct OwnedReport {
    pub directory: File,
    pub file: File,
    accepted: Cell<bool>,
}
impl OwnedReport {
    pub fn new(directory: File, file: File) -> Self {
        Self {
            directory,
            file,
            accepted: Cell::new(false),
        }
    }
}
impl Drop for OwnedReport {
    fn drop(&mut self) {
        if self.accepted.get() {
            return;
        }
        // Resolve through the held output directory, never a replacement at its path.
        let Ok(fd) = rustix::fs::openat(
            &self.directory,
            "report.json",
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) else {
            return;
        };
        let current = File::from(fd);
        let (Ok(named), Ok(held)) = (current.metadata(), self.file.metadata()) else {
            return;
        };
        if named.is_file() && (named.dev(), named.ino()) == (held.dev(), held.ino()) {
            let _ =
                rustix::fs::unlinkat(&self.directory, "report.json", rustix::fs::AtFlags::empty());
        }
    }
}
pub(super) fn publish(
    receipt: &OwnedReport,
    report: &Value,
    mut verify: impl FnMut() -> Result<()>,
    writer: Option<&mut dyn Write>,
) -> Result<Value> {
    let bytes = if writer.is_some() {
        let mut b = serde_json::to_vec(report)?;
        b.push(b'\n');
        Some(b)
    } else {
        None
    };
    // This is the final input/ownership/cancellation check before any success output.
    verify()?;
    if let Some(writer) = writer {
        writer.write_all(bytes.as_deref().expect("serialized publication"))?;
        writer.flush()?;
    }
    // Output cannot be retracted: callers must require successful command exit and
    // the retained disk receipt, never treat stdout alone as an accepted report.
    verify()?;
    receipt.accepted.set(true);
    Ok(report.clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io, path::Path};
    fn report(root: &Path) -> OwnedReport {
        let directory = File::open(root).unwrap();
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(root.join("report.json"))
            .unwrap();
        let mut owned = OwnedReport::new(directory, file);
        owned.file.write_all(b"{\"result\":\"pass\"}\n").unwrap();
        owned
    }
    fn reappear(root: &Path) -> std::path::PathBuf {
        let retired = root.join(super::super::source_policy::RETIRED[0].0);
        fs::create_dir_all(retired.parent().unwrap()).unwrap();
        fs::write(&retired, b"late reappearance").unwrap();
        retired
    }
    #[test]
    fn late_retired_source_and_cancellation_leave_no_owned_pass_or_stdout() {
        for cancel in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let receipt = report(root.path());
            let retired = reappear(root.path());
            let mut stdout = Vec::new();
            let result = publish(
                &receipt,
                &serde_json::json!({"result":"pass"}),
                || {
                    if cancel {
                        Err("driving controller cancelled".into())
                    } else {
                        super::super::source_policy::verify_absent(std::slice::from_ref(&retired))
                    }
                },
                Some(&mut stdout),
            );
            assert!(result.is_err());
            drop(receipt);
            assert!(!root.path().join("report.json").exists());
            assert!(stdout.is_empty());
        }
    }
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn broken_stdout_rolls_back_owned_report_and_preserves_replacement() {
        for foreign in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let receipt = report(root.path());
            if foreign {
                fs::rename(
                    root.path().join("report.json"),
                    root.path().join("moved-owned.json"),
                )
                .unwrap();
                fs::write(root.path().join("report.json"), b"foreign replacement").unwrap();
            }
            let result = publish(
                &receipt,
                &serde_json::json!({"result":"pass"}),
                || Ok(()),
                Some(&mut Broken),
            );
            assert!(result.is_err());
            drop(receipt);
            if foreign {
                assert_eq!(
                    fs::read(root.path().join("report.json")).unwrap(),
                    b"foreign replacement"
                );
            } else {
                assert!(!root.path().join("report.json").exists());
            }
        }
    }
    #[test]
    fn failed_final_verification_preserves_foreign_report_without_stdout() {
        let root = tempfile::tempdir().unwrap();
        let receipt = report(root.path());
        fs::rename(
            root.path().join("report.json"),
            root.path().join("moved-owned.json"),
        )
        .unwrap();
        fs::write(root.path().join("report.json"), b"foreign replacement").unwrap();
        let retired = reappear(root.path());
        let mut stdout = Vec::new();
        let result = publish(
            &receipt,
            &serde_json::json!({"result":"pass"}),
            || super::super::source_policy::verify_absent(std::slice::from_ref(&retired)),
            Some(&mut stdout),
        );
        assert!(result.is_err());
        drop(receipt);
        assert!(stdout.is_empty());
        assert_eq!(
            fs::read(root.path().join("report.json")).unwrap(),
            b"foreign replacement"
        );
    }
    struct BrokenFlush;
    impl Write for BrokenFlush {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }
    #[test]
    fn failed_stdout_flush_leaves_no_owned_pass() {
        let root = tempfile::tempdir().unwrap();
        let receipt = report(root.path());
        assert!(publish(
            &receipt,
            &serde_json::json!({"result":"pass"}),
            || Ok(()),
            Some(&mut BrokenFlush),
        )
        .is_err());
        drop(receipt);
        assert!(!root.path().join("report.json").exists());
    }
    struct MutatingWriter<'a> {
        root: &'a Path,
        cancelled: &'a Cell<bool>,
        cancel: bool,
        during_flush: bool,
        bytes: Vec<u8>,
    }
    impl MutatingWriter<'_> {
        fn mutate(&self) {
            if self.cancel {
                self.cancelled.set(true);
            } else {
                reappear(self.root);
            }
        }
    }
    impl Write for MutatingWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            if !self.during_flush {
                self.mutate();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.during_flush {
                self.mutate();
            }
            Ok(())
        }
    }
    #[test]
    fn successful_stdout_cannot_accept_mutated_sources_or_cancellation() {
        for cancel in [false, true] {
            for during_flush in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let receipt = report(root.path());
                let retired = root.path().join(super::super::source_policy::RETIRED[0].0);
                let cancelled = Cell::new(false);
                let mut writer = MutatingWriter {
                    root: root.path(),
                    cancelled: &cancelled,
                    cancel,
                    during_flush,
                    bytes: Vec::new(),
                };
                let result = publish(
                    &receipt,
                    &serde_json::json!({"result":"pass"}),
                    || {
                        if cancelled.get() {
                            Err("driving controller cancelled".into())
                        } else {
                            super::super::source_policy::verify_absent(std::slice::from_ref(
                                &retired,
                            ))
                        }
                    },
                    Some(&mut writer),
                );
                assert!(result.is_err());
                drop(receipt);
                assert!(!root.path().join("report.json").exists());
                // Already-written bytes are observable, but an error and removed
                // disk receipt prevent them from constituting a successful run.
                assert_eq!(writer.bytes, b"{\"result\":\"pass\"}\n");
            }
        }
    }
    #[test]
    fn verified_publication_keeps_owned_report() {
        let root = tempfile::tempdir().unwrap();
        let receipt = report(root.path());
        let mut stdout = Vec::new();
        publish(
            &receipt,
            &serde_json::json!({"result":"pass"}),
            || Ok(()),
            Some(&mut stdout),
        )
        .unwrap();
        drop(receipt);
        assert!(root.path().join("report.json").is_file());
        assert_eq!(stdout, b"{\"result\":\"pass\"}\n");
    }
}
