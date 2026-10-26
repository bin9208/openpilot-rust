//! Compressed owned .deb fixtures test preparation boundaries only. ELF-shaped
//! bytes are never executed; genuine AGNOS checks use the official packages.
#![cfg(feature = "prepare-capnp")]
use openpilot_runtime_build::{
    assets::{Asset, Download, Plan},
    capnp, digest_file,
    inputs::Tree,
    Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::Cursor,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const PACKAGES: [(&str, &str); 3] = [
    ("capnproto-1-0-1-4-arm64", "capnproto"),
    ("libcapnp-1-0-1-1-0-1-4-arm64", "libcapnp-1.0.1"),
    ("libcapnp-dev-1-0-1-4-arm64", "libcapnp-dev"),
];

struct Fetch {
    blobs: BTreeMap<String, Vec<u8>>,
    calls: RefCell<Vec<String>>,
}
impl Download for Fetch {
    fn fetch(&self, asset: &Asset, path: &Path) -> Result<u64> {
        self.calls.borrow_mut().push(asset.sha256.clone());
        let bytes = self.blobs.get(&asset.sha256).ok_or("owned deb missing")?;
        fs::write(path, bytes)?;
        Ok(bytes.len() as u64)
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
fn append(tar: &mut tar::Builder<Vec<u8>>, path: &str, bytes: &[u8], mode: u32) {
    let mut header = tar::Header::new_gnu();
    header.set_mode(mode);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    tar.append_data(&mut header, path, bytes).unwrap();
}
fn ar_member(ar: &mut Vec<u8>, name: &str, bytes: &[u8]) {
    ar.extend(
        format!(
            "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            0,
            0,
            0,
            "100644",
            bytes.len()
        )
        .bytes(),
    );
    ar.extend_from_slice(bytes);
    if bytes.len() % 2 != 0 {
        ar.push(b'\n');
    }
}
fn deb(package: &str, value: u8, fault: &str) -> Vec<u8> {
    let mut control = tar::Builder::new(Vec::new());
    let architecture = if fault == "architecture" {
        "amd64"
    } else {
        "arm64"
    };
    let content = format!("Package: {package}\nVersion: 1.0.1-4\nArchitecture: {architecture}\nDepends: libc6 (>= 2.38)\n");
    append(&mut control, "./control", content.as_bytes(), 0o644);
    append(&mut control, "./postinst", b"#!/bin/sh\nexit 99\n", 0o755);
    let control = zstd::stream::encode_all(Cursor::new(control.into_inner().unwrap()), 1).unwrap();
    let mut data = tar::Builder::new(Vec::new());
    let mut root = tar::Header::new_gnu();
    root.set_entry_type(tar::EntryType::Directory);
    root.set_mode(0o755);
    root.set_size(0);
    data.append_data(&mut root, "./", std::io::empty()).unwrap();
    if fault != "license" {
        append(
            &mut data,
            &format!("./usr/share/doc/{package}/copyright"),
            b"Owned license fixture\n",
            0o644,
        );
    }
    match package {
        "capnproto" => {
            for name in ["capnp", "capnpc-c++", "capnpc-capnp"] {
                append(&mut data, &format!("./usr/bin/{name}"), &elf(value), 0o755);
            }
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o777);
            header.set_size(0);
            header
                .set_link_name(match fault {
                    "absolute" => "/bin/sh",
                    "directory" => "../include",
                    _ => "capnp",
                })
                .unwrap();
            data.append_data(&mut header, "./usr/bin/capnpc", std::io::empty())
                .unwrap();
        }
        "libcapnp-1.0.1" => {
            for name in ["libcapnp-1.0.1.so", "libkj-1.0.1.so"] {
                append(
                    &mut data,
                    &format!("./usr/lib/aarch64-linux-gnu/{name}"),
                    &elf(value),
                    0o644,
                );
            }
        }
        "libcapnp-dev" => append(
            &mut data,
            "./usr/include/capnp/message.h",
            b"Owned header",
            0o644,
        ),
        _ => unreachable!(),
    }
    if fault == "escape" {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(1);
        let name = b"./usr/../../escape";
        header.as_mut_bytes()[..name.len()].copy_from_slice(name);
        header.set_cksum();
        data.append(&header, b"x".as_slice()).unwrap();
    }
    let mut raw = data.into_inner().unwrap();
    if fault == "unpacked" {
        // Oversized header is rejected before allocating or reading its payload.
        let mut header = tar::Header::new_gnu();
        header.set_path("./usr/huge").unwrap();
        header.set_mode(0o644);
        header.set_size(64 * 1024 * 1024 + 1);
        header.set_cksum();
        raw.splice(0..0, header.as_bytes().iter().copied());
    }
    let mut data = zstd::stream::encode_all(Cursor::new(raw), 1).unwrap();
    if fault == "truncated" {
        data.truncate(data.len() - 3);
    }
    let mut ar = b"!<arch>\n".to_vec();
    ar_member(&mut ar, "debian-binary", b"2.0\n");
    ar_member(&mut ar, "control.tar.zst", &control);
    ar_member(&mut ar, "data.tar.zst", &data);
    if fault == "extra" {
        ar_member(&mut ar, "extra", b"x");
    }
    ar
}

fn fixture(fault: &str) -> (tempfile::TempDir, Plan, Fetch, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("assets");
    let store = temp.path().join("sdk");
    let mut blobs = BTreeMap::new();
    let assets = PACKAGES
        .into_iter()
        .map(|(name, package)| {
            let bytes = deb(package, 7, if package == "capnproto" { fault } else { "" });
            let sha256 = sha(&bytes);
            let asset = Asset {
                url: format!("https://owned.invalid/{name}.deb"),
                sha256: sha256.clone(),
                bytes: bytes.len() as u64,
            };
            blobs.insert(sha256, bytes);
            (name.into(), asset)
        })
        .collect();
    (
        temp,
        Plan {
            schema: 1,
            assets,
            reserve_bytes: 1024 * 1024,
        },
        Fetch {
            blobs,
            calls: RefCell::new(vec![]),
        },
        cache,
        store,
    )
}

#[test]
fn authenticated_packages_repeat_without_transfer_copy_or_source_commit_identity() {
    let (_temp, mut plan, mut fetch, cache, store) = fixture("");
    let first = capnp::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert!(first.copied_bytes > 0 && !first.compiler_executed && !first.runtime_activated);
    assert_eq!(fs::read(first.root.join("usr/bin/capnp")).unwrap(), elf(7));
    assert!(!first.root.join("postinst").exists());
    assert_eq!(
        fs::read_link(first.root.join("usr/bin/capnpc")).unwrap(),
        Path::new("capnp")
    );
    plan.reserve_bytes += 1;
    plan.assets
        .values_mut()
        .for_each(|a| a.url.push_str("?mirror=second"));
    let repeated = capnp::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_eq!(repeated.root, first.root);
    assert_eq!(repeated.tree_sha256, first.tree_sha256);
    assert_eq!(repeated.copied_bytes, 0);
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert!(repeated
        .assets
        .values()
        .all(|a| a.transferred_bytes == 0 && a.stored_bytes == 0));
    let changed = deb("libcapnp-1.0.1", 9, "");
    let hash = sha(&changed);
    let a = plan.assets.get_mut(PACKAGES[1].0).unwrap();
    a.sha256 = hash.clone();
    a.bytes = changed.len() as u64;
    fetch.blobs.insert(hash, changed);
    let next = capnp::prepare(&plan, &cache, &store, &fetch).unwrap();
    assert_ne!(next.root, first.root);
    assert_eq!(fetch.calls.borrow().len(), 4);
    assert_eq!(next.assets[PACKAGES[0].0].transferred_bytes, 0);
    assert_eq!(next.assets[PACKAGES[2].0].transferred_bytes, 0);
    assert_eq!(
        fs::read(
            first
                .root
                .join("usr/lib/aarch64-linux-gnu/libcapnp-1.0.1.so")
        )
        .unwrap(),
        elf(7)
    );
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
}

#[test]
fn rewritten_receipts_cannot_certify_mutated_compiler_and_other_generations_survive() {
    let (temp, plan, fetch, cache, store) = fixture("");
    let first = capnp::prepare(&plan, &cache, &store, &fetch).unwrap();
    let normal = temp.path().join("normal-runtime");
    fs::write(&normal, "keep current logs and runtime").unwrap();
    let compiler = first.root.join("usr/bin/capnp");
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&compiler, elf(9)).unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o555)).unwrap();
    let tree = Tree::capture(&first.root).unwrap();
    let mut bytes = serde_json::to_vec_pretty(&tree).unwrap();
    bytes.push(b'\n');
    fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&first.tree_receipt, bytes).unwrap();
    let receipt = first.tree_receipt.parent().unwrap().join("receipt.json");
    let mut r: serde_json::Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    r["tree_sha256"] = digest_file(&first.tree_receipt).unwrap().into();
    fs::set_permissions(&receipt, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&receipt, serde_json::to_vec(&r).unwrap()).unwrap();
    assert!(capnp::prepare(&plan, &cache, &store, &fetch)
        .unwrap_err()
        .to_string()
        .contains("authenticated"));
    assert_eq!(fetch.calls.borrow().len(), 3);
    assert_eq!(
        fs::read_to_string(normal).unwrap(),
        "keep current logs and runtime"
    );
}

#[test]
fn unsafe_paths_links_package_identity_unpacked_limits_and_truncation_do_not_publish() {
    for fault in [
        "escape",
        "absolute",
        "directory",
        "architecture",
        "license",
        "unpacked",
        "truncated",
        "extra",
    ] {
        let (temp, plan, fetch, cache, store) = fixture(fault);
        assert!(
            capnp::prepare(&plan, &cache, &store, &fetch).is_err(),
            "{fault}"
        );
        assert!(!temp.path().join("escape").exists());
        assert!(fs::read_dir(&store).unwrap().all(|e| !e
            .unwrap()
            .path()
            .join("generation")
            .exists()));
    }
}

#[test]
fn low_capacity_busy_owner_and_corrupt_raw_cache_preserve_verified_prefix() {
    let (_temp, mut plan, fetch, cache, store) = fixture("");
    plan.reserve_bytes = u64::MAX;
    assert!(capnp::prepare(&plan, &cache, &store, &fetch).is_err());
    assert!(fetch.calls.borrow().is_empty());
    plan.reserve_bytes = 1024 * 1024;
    let first = capnp::prepare(&plan, &cache, &store, &fetch).unwrap();
    let lock = fs::File::open(
        first
            .tree_receipt
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lock"),
    )
    .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    assert!(capnp::prepare(&plan, &cache, &store, &fetch).is_err());
    drop(lock);
    let raw = &first.assets[PACKAGES[0].0].path;
    fs::set_permissions(raw, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(raw, vec![b'x'; plan.assets[PACKAGES[0].0].bytes as usize]).unwrap();
    assert!(capnp::prepare(&plan, &cache, &store, &fetch).is_err());
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    assert_eq!(fetch.calls.borrow().len(), 3);
}
