//! Owned compressed Rust-layout fixtures for archive and reuse boundaries.
//! Header-shaped test programs are never executed and are not compiler proof;
//! the genuine AGNOS workflow separately runs the official compiler and Cargo.
#![cfg(feature = "prepare-rust")]
use openpilot_runtime_build::{
    assets::{Asset, Download},
    digest_file,
    inputs::Tree,
    toolchain::{self, Component, Plan},
    Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

struct Fetch {
    blobs: BTreeMap<String, Vec<u8>>,
    calls: RefCell<Vec<String>>,
}
impl Download for Fetch {
    fn fetch(&self, asset: &Asset, path: &Path) -> Result<u64> {
        self.calls.borrow_mut().push(asset.sha256.clone());
        let bytes = self
            .blobs
            .get(&asset.sha256)
            .ok_or("owned download missing")?;
        fs::write(path, bytes)?;
        Ok(bytes.len() as u64)
    }
}

fn elf(value: u8) -> Vec<u8> {
    let mut bytes = vec![0; 80];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[52..54].copy_from_slice(&64_u16.to_le_bytes());
    bytes[79] = value;
    bytes
}

fn archive(name: &str, value: u8, link: Option<&str>, unsafe_path: bool) -> Vec<u8> {
    let root = format!("{name}-1.94.0-aarch64-unknown-linux-gnu");
    let component = if name == "rust-std" {
        "rust-std-aarch64-unknown-linux-gnu"
    } else {
        name
    };
    let mut tar = tar::Builder::new(xz2::write::XzEncoder::new(Vec::new(), 1));
    let mut add = |path: &str, bytes: &[u8], mode: u32| {
        let mut header = tar::Header::new_gnu();
        header.set_mode(mode);
        header.set_size(bytes.len() as u64);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        tar.append_data(&mut header, path, bytes).unwrap();
    };
    for license in ["LICENSE-APACHE", "LICENSE-MIT", "COPYRIGHT"] {
        add(
            &format!("{root}/{license}"),
            b"Owned license fixture\n",
            0o644,
        );
    }
    add(
        &format!("{root}/install.sh"),
        b"#!/bin/sh\nexit 99\n",
        0o755,
    );
    add(
        &format!("{root}/{component}/manifest.in"),
        b"Owned component manifest\n",
        0o644,
    );
    match name {
        "cargo" => add(&format!("{root}/{component}/bin/cargo"), &elf(value), 0o755),
        "rustc" => {
            add(&format!("{root}/{component}/bin/rustc"), &elf(value), 0o755);
            add(&format!("{root}/{component}/bin/rustdoc"), &elf(5), 0o755);
            add(
                &format!("{root}/{component}/lib/real.so"),
                b"Owned library fixture",
                0o644,
            );
        }
        "rust-std" => add(
            &format!(
                "{root}/{component}/lib/rustlib/aarch64-unknown-linux-gnu/lib/libstd-owned.rlib"
            ),
            b"Owned standard-library fixture",
            0o644,
        ),
        _ => unreachable!(),
    }
    if unsafe_path {
        // Deliberately bypass the tar builder's normal path admission to test
        // the production reader against an authenticated but unsafe archive.
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        header.set_entry_type(tar::EntryType::Regular);
        let path = format!("{root}/{component}/../../escape");
        header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        tar.append(&header, b"x".as_slice()).unwrap();
    }
    if let Some(target) = link {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o777);
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name(target).unwrap();
        tar.append_data(
            &mut header,
            format!("{root}/{component}/lib/alias.so"),
            std::io::empty(),
        )
        .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

fn fixture(
    link: Option<&str>,
    unsafe_path: bool,
) -> (tempfile::TempDir, Plan, Fetch, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("assets");
    let store = temp.path().join("toolchains");
    let mut blobs = BTreeMap::new();
    let components = ["cargo", "rust-std", "rustc"]
        .into_iter()
        .map(|name| {
            let bytes = archive(
                name,
                7,
                if name == "rustc" { link } else { None },
                unsafe_path && name == "rustc",
            );
            let sha = format!("{:x}", Sha256::digest(&bytes));
            let component = Component {
                asset: Asset {
                    url: format!("https://owned.invalid/{name}.tar.xz"),
                    sha256: sha.clone(),
                    bytes: bytes.len() as u64,
                },
                archive_root: format!("{name}-1.94.0-aarch64-unknown-linux-gnu"),
                component: if name == "rust-std" {
                    "rust-std-aarch64-unknown-linux-gnu".into()
                } else {
                    name.into()
                },
                maximum_unpacked_bytes: 1024 * 1024,
            };
            blobs.insert(sha, bytes);
            (name.into(), component)
        })
        .collect();
    let plan = Plan {
        schema: 1,
        version: "1.94.0".into(),
        target: "aarch64-unknown-linux-gnu".into(),
        source_manifest_url: "https://static.rust-lang.org/dist/channel-rust-1.94.0.toml".into(),
        source_manifest_sha256: "a".repeat(64),
        reserve_bytes: 1024 * 1024,
        components,
    };
    (
        temp,
        plan,
        Fetch {
            blobs,
            calls: RefCell::new(vec![]),
        },
        cache,
        store,
    )
}

// A larger declared dictionary can decode the same tiny LZMA2 payload. Change
// only that property and its Block Header CRC32, avoiding a huge test encoder.
// XZ format sections 3.1.7, 5.3.1 and 6: https://tukaani.org/xz/xz-file-format.txt
fn dictionary(mut bytes: Vec<u8>, property: u8) -> Vec<u8> {
    assert_eq!(&bytes[..6], b"\xfd7zXZ\0");
    assert_eq!(&bytes[13..16], &[0, 0x21, 1]);
    let end = 12 + (usize::from(bytes[12]) + 1) * 4;
    bytes[16] = property;
    let mut crc = u32::MAX;
    for byte in &bytes[12..end - 4] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    bytes[end - 4..end].copy_from_slice(&(!crc).to_le_bytes());
    bytes
}

fn set_dictionary(plan: &mut Plan, fetch: &mut Fetch, name: &str, property: u8) {
    let component = plan.components.get_mut(name).unwrap();
    let bytes = dictionary(fetch.blobs[&component.asset.sha256].clone(), property);
    let sha = format!("{:x}", Sha256::digest(&bytes));
    component.asset.sha256 = sha.clone();
    component.asset.bytes = bytes.len() as u64;
    fetch.blobs.insert(sha, bytes);
}

#[test]
fn pinned_profile_dictionary_has_decoder_headroom_but_oversized_stream_preserves_prior_toolchains()
{
    let (temp, mut plan, mut fetch, cache, store) = fixture(None, false);
    let old_plan = plan.clone();
    let old = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    let log = temp.path().join("previous-runtime.log");
    fs::write(&log, b"previous runtime and logs retained").unwrap();
    // Match the official Cargo (48 MiB) and rust-std/rustc (128 MiB) headers.
    for (name, property) in [("cargo", 0x1b), ("rust-std", 0x1e), ("rustc", 0x1e)] {
        set_dictionary(&mut plan, &mut fetch, name, property);
    }
    let bytes = &fetch.blobs[&plan.components["rust-std"].asset.sha256];
    let mut legacy_limit = xz2::read::XzDecoder::new_stream(
        bytes.as_slice(),
        xz2::stream::Stream::new_stream_decoder(128 * 1024 * 1024, 0).unwrap(),
    );
    let error = legacy_limit.read_to_end(&mut Vec::new()).unwrap_err();
    assert!(error.to_string().contains("memory limit"), "{error}");
    drop(legacy_limit);
    let first = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert!(!first.compiler_executed && !first.runtime_activated);
    assert_eq!(fs::read(first.root.join("bin/rustc")).unwrap(), elf(7));
    let transfers = fetch.calls.borrow().len();
    let repeated = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_eq!(repeated.root, first.root);
    assert_eq!(repeated.copied_bytes, 0);
    assert_eq!(fetch.calls.borrow().len(), transfers);
    assert!(repeated.assets.values().all(|a| a.transferred_bytes == 0));
    let published = || {
        fs::read_dir(&store)
            .unwrap()
            .filter(|p| p.as_ref().unwrap().path().join("generation").exists())
            .count()
    };
    let before = published();
    // 512 MiB is above the finite decoder limit and must fail before publishing.
    set_dictionary(&mut plan, &mut fetch, "rustc", 0x22);
    let error = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap_err();
    assert!(error.to_string().contains("decoder budget"), "{error}");
    assert!(error.to_string().contains("memory limit"), "{error}");
    assert_eq!(published(), before);
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    assert_eq!(
        toolchain::prepare(&old_plan, &cache, &store, &fetch)
            .unwrap()
            .root,
        old.root
    );
    assert_eq!(
        fs::read(log).unwrap(),
        b"previous runtime and logs retained"
    );
}

#[test]
fn authenticated_archives_install_without_scripts_and_reuse_transfers_and_copies_zero() {
    let (_temp, mut plan, mut fetch, cache, store) = fixture(Some("real.so"), false);
    let first = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert!(first.copied_bytes > 0);
    assert!(!first.compiler_executed && !first.runtime_activated);
    assert_eq!(fs::read(first.root.join("bin/rustc")).unwrap(), elf(7));
    assert!(!first.root.join("install.sh").exists());
    assert_eq!(
        fs::metadata(first.root.join("bin/cargo"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o555
    );
    assert_eq!(
        fs::read_link(first.root.join("lib/alias.so")).unwrap(),
        Path::new("real.so")
    );
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    for name in ["cargo", "rust-std", "rustc"] {
        assert!(first
            .root
            .join("share/licenses/rust")
            .join(name)
            .join("LICENSE-MIT")
            .is_file());
    }
    let repeated = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert_eq!(repeated.root, first.root);
    assert_eq!(repeated.tree_sha256, first.tree_sha256);
    assert_eq!(repeated.copied_bytes, 0);
    assert!(repeated
        .assets
        .values()
        .all(|a| a.transferred_bytes == 0 && a.stored_bytes == 0));
    // Space-policy adjustments do not change compiler identity or rewrite files.
    plan.reserve_bytes += 1;
    plan.components
        .get_mut("rustc")
        .unwrap()
        .maximum_unpacked_bytes += 1;
    assert_eq!(
        toolchain::prepare(&plan, &cache, &store, &fetch)
            .unwrap()
            .root,
        first.root
    );
    let changed = archive("rustc", 9, Some("real.so"), false);
    let sha = format!("{:x}", Sha256::digest(&changed));
    let compiler = plan.components.get_mut("rustc").unwrap();
    compiler.asset.sha256 = sha.clone();
    compiler.asset.bytes = changed.len() as u64;
    fetch.blobs.insert(sha, changed);
    let next = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_ne!(next.root, first.root);
    assert_eq!(fetch.calls.borrow().len(), 4);
    assert_eq!(next.assets["cargo"].transferred_bytes, 0);
    assert_eq!(next.assets["rust-std"].transferred_bytes, 0);
    assert_eq!(fs::read(first.root.join("bin/rustc")).unwrap(), elf(7));
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
}

#[test]
fn rewriting_local_receipts_cannot_certify_a_modified_compiler_and_preserves_normal_runtime() {
    let (temp, plan, fetch, cache, store) = fixture(None, false);
    let first = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    let normal = temp.path().join("normal-runtime-log");
    fs::write(&normal, b"normal runtime and logs retained").unwrap();
    let compiler = first.root.join("bin/rustc");
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&compiler, elf(8)).unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o555)).unwrap();
    let forged = Tree::capture(&first.root).unwrap();
    fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o644)).unwrap();
    let mut bytes = serde_json::to_vec_pretty(&forged).unwrap();
    bytes.push(b'\n');
    fs::write(&first.tree_receipt, bytes).unwrap();
    fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o444)).unwrap();
    let receipt_path = first.tree_receipt.parent().unwrap().join("receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt["tree_sha256"] = digest_file(&first.tree_receipt).unwrap().into();
    fs::set_permissions(&receipt_path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch)
        .unwrap_err()
        .to_string()
        .contains("authenticated"));
    assert_eq!(
        fs::read(&normal).unwrap(),
        b"normal runtime and logs retained"
    );
    assert_eq!(fs::read(&compiler).unwrap(), elf(8));
}

#[test]
fn unsafe_links_paths_low_capacity_and_busy_owner_never_publish_or_replace_a_toolchain() {
    for link in ["../../outside", "/etc/passwd", "alias.so", "missing.so"] {
        let (_temp, plan, fetch, cache, store) = fixture(Some(link), false);
        assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
        assert!(!fs::read_dir(&store).unwrap().any(|p| p
            .unwrap()
            .path()
            .join("generation")
            .exists()));
    }
    let (_temp, plan, fetch, cache, store) = fixture(None, true);
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
    let (_temp, mut plan, fetch, cache, store) = fixture(None, false);
    plan.reserve_bytes = u64::MAX;
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
    assert!(fetch.calls.borrow().is_empty());
    plan.reserve_bytes = 0;
    let first = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    let object = first.root.parent().unwrap().parent().unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(object.join("lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
    assert_eq!(fs::read(first.root.join("bin/rustc")).unwrap(), elf(7));
}

#[test]
fn corrupt_archive_and_incomplete_std_cannot_publish_and_preserve_an_older_compiler() {
    let (_temp, mut plan, mut fetch, cache, store) = fixture(None, false);
    let first = toolchain::prepare(&plan, &cache, &store, &fetch).unwrap();
    let changed = archive("rustc", 9, None, false);
    let sha = format!("{:x}", Sha256::digest(&changed));
    let compiler = plan.components.get_mut("rustc").unwrap();
    compiler.asset.sha256 = sha.clone();
    compiler.asset.bytes = changed.len() as u64;
    let mut corrupt = changed.clone();
    corrupt[0] ^= 1;
    fetch.blobs.insert(sha.clone(), corrupt);
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    assert_eq!(fs::read(first.root.join("bin/rustc")).unwrap(), elf(7));
    fetch.blobs.insert(sha, changed);
    // Admission applies even to an authenticated archive when its unpacked
    // content exceeds the declared per-component resource limit.
    plan.components
        .get_mut("rust-std")
        .unwrap()
        .maximum_unpacked_bytes = 1;
    assert!(toolchain::prepare(&plan, &cache, &store, &fetch).is_err());
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
}
