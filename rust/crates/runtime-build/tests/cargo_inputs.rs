//! Owned published-crate-shaped archives. These preparation tests execute no
//! third-party code; the actual Cargo regression lives in cargo_reuse.rs.
#![cfg(feature = "prepare-cargo")]
use openpilot_runtime_build::{
    assets::{Asset, Download},
    cargo_inputs::{self, Package, Plan},
    digest_file,
    inputs::Tree,
    Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Fetch {
    blobs: BTreeMap<String, Vec<u8>>,
    calls: RefCell<Vec<String>>,
}
impl Download for Fetch {
    fn fetch(&self, asset: &Asset, path: &Path) -> Result<u64> {
        self.calls.borrow_mut().push(asset.sha256.clone());
        let bytes = &self.blobs[&asset.sha256];
        fs::write(path, bytes)?;
        Ok(bytes.len() as u64)
    }
}
fn archive(name: &str, value: u8, fault: &str) -> (Vec<u8>, u64, u64) {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(encoder);
    let mut total = 0;
    let mut members = 0;
    let manifest = format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n");
    let code = format!("pub fn answer()->u8{{{value}}}\n");
    for (path, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", code.as_bytes()),
        ("LICENSE", b"Owned license fixture\n".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(if fault == "setid" { 0o4644 } else { 0o644 });
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        tar.append_data(&mut header, format!("{name}-0.1.0/{path}"), bytes)
            .unwrap();
        total += bytes.len() as u64;
        members += 1;
    }
    if fault == "escape" {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        let path = format!("{name}-0.1.0/../../escape");
        header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        tar.append(&header, b"x".as_slice()).unwrap();
        total += 1;
        members += 1;
    }
    if fault == "link" {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o777);
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("/etc/passwd").unwrap();
        tar.append_data(
            &mut header,
            format!("{name}-0.1.0/foreign"),
            std::io::empty(),
        )
        .unwrap();
    }
    if fault == "checksum" {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(2);
        tar.append_data(
            &mut header,
            format!("{name}-0.1.0/.cargo-checksum.json"),
            b"{}".as_slice(),
        )
        .unwrap();
        total += 2;
        members += 1;
    }
    let mut bytes = tar.into_inner().unwrap().finish().unwrap();
    if fault == "truncated" {
        bytes.truncate(bytes.len() - 3);
    }
    (bytes, total, members)
}
fn write_lock(plan: &mut Plan, lock: &Path) {
    let mut text = "version = 4\n".to_owned();
    for p in &plan.packages {
        text.push_str(&format!("\n[[package]]\nname={:?}\nversion={:?}\nsource=\"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum={:?}\n",p.name,p.version,p.asset.sha256));
    }
    fs::write(lock, text).unwrap();
    plan.cargo_lock_sha256 = digest_file(lock).unwrap();
}
fn fixture(fault: &str) -> (tempfile::TempDir, Plan, Fetch, PathBuf, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("assets");
    let store = temp.path().join("sources");
    let lock = temp.path().join("Cargo.lock");
    let mut packages = Vec::new();
    let mut blobs = BTreeMap::new();
    for name in ["owned-alpha", "owned-beta"] {
        let (bytes, unpacked_bytes, members) =
            archive(name, 42, if name == "owned-alpha" { fault } else { "" });
        let hash = sha(&bytes);
        packages.push(Package {
            name: name.into(),
            version: "0.1.0".into(),
            asset: Asset {
                url: format!("https://owned.invalid/{name}.crate"),
                sha256: hash.clone(),
                bytes: bytes.len() as u64,
            },
            unpacked_bytes,
            members,
        });
        blobs.insert(hash, bytes);
    }
    let mut plan = Plan {
        schema: 1,
        cargo_lock_sha256: String::new(),
        reserve_bytes: 1024 * 1024,
        packages,
    };
    write_lock(&mut plan, &lock);
    (
        temp,
        plan,
        Fetch {
            blobs,
            calls: RefCell::new(vec![]),
        },
        cache,
        store,
        lock,
    )
}

#[test]
fn repeat_copies_and_transfers_zero_and_new_lock_hardlinks_unchanged_sources() {
    let (_temp, mut plan, mut fetch, cache, store, lock) = fixture("");
    let first = cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).unwrap();
    assert_eq!(fetch.calls.borrow().len(), 2);
    assert!(first.copied_bytes > 0 && first.linked_bytes > 0);
    assert!(!first.compiler_executed && !first.runtime_activated);
    let repeated = cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).unwrap();
    assert_eq!(repeated.root, first.root);
    assert_eq!(repeated.copied_bytes, 0);
    assert_eq!(repeated.linked_bytes, 0);
    assert!(repeated
        .assets
        .values()
        .all(|r| r.transferred_bytes == 0 && r.stored_bytes == 0));
    assert_eq!(fetch.calls.borrow().len(), 2);
    let (bytes, unpacked_bytes, members) = archive("owned-alpha", 43, "");
    let hash = sha(&bytes);
    let p = &mut plan.packages[0];
    p.asset.sha256 = hash.clone();
    p.asset.bytes = bytes.len() as u64;
    p.unpacked_bytes = unpacked_bytes;
    p.members = members;
    fetch.blobs.insert(hash, bytes);
    write_lock(&mut plan, &lock);
    let changed = cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).unwrap();
    assert_ne!(changed.root, first.root);
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert_eq!(
        changed.assets[&plan.packages[1].asset.sha256].transferred_bytes,
        0
    );
    let beta = "owned-beta-0.1.0/src/lib.rs";
    assert_eq!(
        fs::metadata(first.vendor.join(beta)).unwrap().ino(),
        fs::metadata(changed.vendor.join(beta)).unwrap().ino()
    );
    assert!(
        fs::read_to_string(first.vendor.join("owned-alpha-0.1.0/src/lib.rs"))
            .unwrap()
            .contains("42")
    );
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
}

fn forge(root: &Path, receipt: &Path) {
    let mut bytes = serde_json::to_vec_pretty(&Tree::capture(root).unwrap()).unwrap();
    bytes.push(b'\n');
    fs::set_permissions(receipt, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(receipt, bytes).unwrap();
}
#[test]
fn rewriting_both_package_and_set_receipts_cannot_certify_mutated_source() {
    let (temp, plan, fetch, cache, store, lock) = fixture("");
    let first = cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).unwrap();
    let normal = temp.path().join("running-runtime-and-logs");
    fs::write(&normal, "preserved").unwrap();
    let source = first.vendor.join("owned-alpha-0.1.0/src/lib.rs");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&source, "pub fn answer()->u8{43}\n").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o444)).unwrap();
    let object = store
        .join("crates")
        .join(&plan.packages[0].asset.sha256)
        .join("generation");
    forge(&object.join("tree"), &object.join("tree.json"));
    forge(&first.root, &first.tree_receipt);
    assert!(cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).is_err());
    assert_eq!(fs::read_to_string(normal).unwrap(), "preserved");
    assert_eq!(fetch.calls.borrow().len(), 2);
}

#[test]
fn unsafe_archive_lock_set_and_declared_bounds_never_publish_a_set() {
    for fault in [
        "escape",
        "link",
        "checksum",
        "setid",
        "truncated",
        "unpacked",
        "members",
        "missing-package",
    ] {
        let (temp, mut plan, fetch, cache, store, lock) = fixture(fault);
        match fault {
            "unpacked" => plan.packages[0].unpacked_bytes -= 1,
            "members" => plan.packages[0].members += 1,
            "missing-package" => {
                plan.packages.pop();
            }
            _ => (),
        }
        assert!(
            cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).is_err(),
            "{fault}"
        );
        assert!(!temp.path().join("escape").exists());
        let sets = store.join("sets");
        if sets.exists() {
            assert!(fs::read_dir(sets).unwrap().all(|e| !e
                .unwrap()
                .path()
                .join("generation")
                .exists()));
        }
    }
}

#[test]
fn low_capacity_busy_set_and_corrupt_raw_cache_preserve_verified_inputs() {
    let (_temp, mut plan, fetch, cache, store, lock) = fixture("");
    plan.reserve_bytes = u64::MAX;
    assert!(cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).is_err());
    assert!(fetch.calls.borrow().is_empty());
    plan.reserve_bytes = 1024 * 1024;
    let first = cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).unwrap();
    let owner = fs::File::open(
        first
            .tree_receipt
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lock"),
    )
    .unwrap();
    rustix::fs::flock(&owner, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    assert!(cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).is_err());
    drop(owner);
    let raw = &first.assets[&plan.packages[0].asset.sha256].path;
    fs::set_permissions(raw, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(raw, vec![b'x'; plan.packages[0].asset.bytes as usize]).unwrap();
    assert!(cargo_inputs::prepare(&plan, &lock, &cache, &store, &fetch).is_err());
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    assert_eq!(fetch.calls.borrow().len(), 2);
}
