//! Owned gzip archives test admission and preservation. Header-shaped ELF test
//! bytes are never executed; Actions separately runs the real pinned ARM Python.
#![cfg(feature = "prepare-python")]
use openpilot_runtime_build::{
    assets::{Asset, Download},
    inputs::Tree,
    python::{self, Plan},
    Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fs::{self, OpenOptions},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn archive(case: &str, value: u8) -> Vec<u8> {
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    let mut elf = vec![0; 80];
    elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    elf[18..20]
        .copy_from_slice(&(if case == "wrong-elf" { 62_u16 } else { 183_u16 }).to_le_bytes());
    elf[20..24].copy_from_slice(&1_u32.to_le_bytes());
    elf[52..54].copy_from_slice(&64_u16.to_le_bytes());
    elf[79] = value;
    for (path, bytes, mode) in [
        ("bin/python3.12", elf.as_slice(), 0o755),
        ("include/python3.12/Python.h", b"owned header", 0o644),
        ("lib/python3.12/os.py", b"# owned stdlib fixture", 0o644),
        (
            "lib/python3.12/encodings/__init__.py",
            b"# owned encoding fixture",
            0o644,
        ),
        ("LICENSE.txt", b"owned license fixture", 0o644),
    ] {
        if (case == "missing" && path == "lib/python3.12/os.py")
            || (case == "no-notice" && path == "LICENSE.txt")
        {
            continue;
        }
        let mut header = tar::Header::new_gnu();
        header.set_mode(if case == "special" && path == "bin/python3.12" {
            0o4755
        } else {
            mode
        });
        header.set_size(bytes.len() as u64);
        tar.append_data(&mut header, format!("python/{path}"), bytes)
            .unwrap();
        if case == "duplicate" && path == "LICENSE.txt" {
            tar.append_data(&mut header, format!("python/{path}"), bytes)
                .unwrap();
        }
    }
    let long = format!(
        "python/lib/python3.12/{}/owned.py",
        "long_package_".repeat(12)
    );
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(1);
    tar.append_data(&mut header, long, b"x".as_slice()).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o777);
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Symlink);
    header
        .set_link_name(if case == "redirect" {
            "../../../escape"
        } else if case == "cycle" {
            "python3"
        } else {
            "python3.12"
        })
        .unwrap();
    tar.append_data(&mut header, "python/bin/python3", std::io::empty())
        .unwrap();
    if case == "traversal" {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        let path = b"python/../../escape";
        header.as_mut_bytes()[..path.len()].copy_from_slice(path);
        header.set_cksum();
        tar.append(&header, b"x".as_slice()).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}
struct Fetch {
    bytes: Vec<u8>,
    calls: RefCell<usize>,
    bad: bool,
}
impl Download for Fetch {
    fn fetch(&self, asset: &Asset, destination: &Path) -> Result<u64> {
        *self.calls.borrow_mut() += 1;
        let mut bytes = self.bytes.clone();
        if self.bad {
            bytes[0] ^= 1;
        }
        fs::write(destination, bytes)?;
        Ok(asset.bytes)
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    cache: PathBuf,
    store: PathBuf,
    plan: Plan,
    fetch: Fetch,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut plan: Plan =
            serde_json::from_str(include_str!("../../../native-python-toolchain.json")).unwrap();
        let bytes = archive("valid", 3);
        plan.asset.sha256 = sha(&bytes);
        plan.asset.bytes = bytes.len() as u64;
        Self {
            cache: temp.path().join("cache"),
            store: temp.path().join("python"),
            _temp: temp,
            plan,
            fetch: Fetch {
                bytes,
                calls: RefCell::new(0),
                bad: false,
            },
        }
    }
    fn replace(&mut self, bytes: Vec<u8>) {
        self.plan.asset.sha256 = sha(&bytes);
        self.plan.asset.bytes = bytes.len() as u64;
        self.fetch.bytes = bytes;
    }
}

#[test]
fn reuse_and_revised_growth_estimate_keep_the_same_content_without_download_or_copy() {
    let mut f = Fixture::new();
    let first = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
    assert!(first.copied_bytes > 0 && first.asset.transferred_bytes > 0);
    assert!(!first.installer_executed && !first.interpreter_executed && !first.runtime_activated);
    let inode = fs::metadata(&first.python).unwrap().ino();
    f.plan.reserve_bytes += 1024 * 1024;
    let repeat = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
    assert_eq!(first.root, repeat.root);
    assert_eq!(first.tree_sha256, repeat.tree_sha256);
    assert_eq!(
        repeat.copied_bytes
            + repeat.asset.transferred_bytes
            + repeat.asset.stored_bytes
            + repeat.expected_additional_bytes,
        0
    );
    assert_eq!(fs::metadata(&repeat.python).unwrap().ino(), inode);
    assert_eq!(*f.fetch.calls.borrow(), 1);
    assert_eq!(
        fs::metadata(&first.python).unwrap().permissions().mode() & 0o7777,
        0o555
    );
    assert_eq!(
        fs::read_link(first.root.join("bin/python3")).unwrap(),
        Path::new("python3.12")
    );
}

#[test]
fn changed_toolchain_gets_a_separate_generation_without_changing_the_previous_one() {
    let mut f = Fixture::new();
    let first = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
    let bytes = fs::read(&first.python).unwrap();
    f.replace(archive("valid", 9));
    let changed = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
    assert_ne!(first.root, changed.root);
    assert_ne!(first.tree_sha256, changed.tree_sha256);
    assert_eq!(*f.fetch.calls.borrow(), 2);
    assert_eq!(fs::read(&first.python).unwrap(), bytes);
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
}

#[test]
fn authenticated_unsafe_incomplete_or_duplicate_archives_and_bad_download_preserve_old_inputs() {
    for case in [
        "missing",
        "special",
        "duplicate",
        "redirect",
        "traversal",
        "bad-download",
        "wrong-elf",
        "no-notice",
        "cycle",
    ] {
        let mut f = Fixture::new();
        let first = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
        f.replace(archive(case, 9));
        f.fetch.bad = case == "bad-download";
        assert!(
            python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).is_err(),
            "{case}"
        );
        Tree::read(&first.tree_receipt)
            .unwrap()
            .verify(&first.root)
            .unwrap();
        assert!(!f._temp.path().join("escape").exists());
        assert_eq!(
            fs::read_dir(&f.store)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().join("generation").exists())
                .count(),
            1
        );
    }
}

#[test]
fn wrong_plan_is_rejected_before_cache_or_download() {
    for case in ["version", "target", "release", "url", "reserve"] {
        let mut f = Fixture::new();
        match case {
            "version" => f.plan.version = "3.13.0".into(),
            "target" => f.plan.target = "x86_64-unknown-linux-gnu".into(),
            "release" => f.plan.release = "unversioned".into(),
            "url" => f.plan.asset.url = "https://example.invalid/python.tar.gz".into(),
            "reserve" => f.plan.reserve_bytes = 0,
            _ => unreachable!(),
        }
        assert!(python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).is_err());
        assert!(!f.cache.exists() && !f.store.exists());
        assert_eq!(*f.fetch.calls.borrow(), 0);
    }
}

#[test]
fn corrupt_receipt_prefix_or_consumed_archive_cannot_repair_itself_and_lock_conflicts_preserve_data(
) {
    for case in ["receipt", "prefix", "archive", "extra", "lock"] {
        let f = Fixture::new();
        let first = python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).unwrap();
        let mut lock = None;
        match case {
            "receipt" => {
                fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o644))
                    .unwrap();
                fs::write(&first.tree_receipt, "{}").unwrap();
            }
            "prefix" | "archive" => {
                let path = if case == "prefix" {
                    &first.python
                } else {
                    &first.asset.path
                };
                fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
                fs::write(path, "corrupted").unwrap();
            }
            "extra" => fs::write(first.root.join("unexpected"), "untrusted").unwrap(),
            "lock" => {
                let file = OpenOptions::new()
                    .write(true)
                    .open(first.root.parent().unwrap().parent().unwrap().join("lock"))
                    .unwrap();
                rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
                    .unwrap();
                lock = Some(file);
            }
            _ => unreachable!(),
        }
        assert!(
            python::prepare(&f.plan, &f.cache, &f.store, &f.fetch).is_err(),
            "{case}"
        );
        assert_eq!(*f.fetch.calls.borrow(), 1);
        if case == "lock" {
            Tree::read(&first.tree_receipt)
                .unwrap()
                .verify(&first.root)
                .unwrap();
            drop(lock);
            assert_eq!(
                python::prepare(&f.plan, &f.cache, &f.store, &f.fetch)
                    .unwrap()
                    .copied_bytes,
                0
            );
        }
    }
}
