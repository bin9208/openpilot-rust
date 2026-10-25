//! Stage repository-owned synthetic protocol observations; never run historical Python.
use crate::{disk_space, seal::Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub const DIRECTORY: &str = "rust/crates/dev-tools/tests/fixtures/pandad-protocol-original";
pub const MANIFEST_SHA256: &str =
    "bb63c7f4cf0a39971ae8b2306294179387fad1474b5ec0e0cce5c298de486902";
pub const SOURCE_COMMIT: &str = "e7bd8a2fc9b1e3b9984043214779583bc1237e5e";
const LEDGER_SHA256: &str = "29b51c0f3f7460d94b4915e442a4de8190b32145365aee3f085f85a13d7a3f02";
const BLOB_SHA256: &str = "a8100c2bd7ef4fc3d58f122befceb9f81ff64cc09823d4cff725f9692b269b61";
const MAX_PACKED: u64 = 32 * 1024 * 1024;
const MAX_UNPACKED: u64 = 128 * 1024 * 1024;
const LEDGER: &[u8] = include_bytes!("../tests/fixtures/pandad-protocol-original/fixture.json");

#[derive(Deserialize)]
struct Seal {
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Ledger {
    blob_sha256: String,
    blob_bytes: u64,
    unpacked_bytes: u64,
    files: BTreeMap<String, Seal>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn relative(name: &str) -> Result<&Path> {
    if name.is_empty()
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || !Path::new(name)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
    {
        return Err("unsafe protocol fixture path".into());
    }
    Ok(Path::new(name))
}
fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>> {
    for parent in path.ancestors().skip(1) {
        if !fs::symlink_metadata(parent)?.file_type().is_dir() {
            return Err("protocol fixture parent is not a real directory".into());
        }
    }
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit {
        return Err("protocol fixture input is nonregular or oversized".into());
    }
    let mut bytes = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut bytes)?;
    let identity = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    if bytes.len() as u64 != before.len()
        || identity(&before) != identity(&file.metadata()?)
        || identity(&before) != identity(&fs::symlink_metadata(path)?)
    {
        return Err("protocol fixture input changed while reading".into());
    }
    Ok(bytes)
}
fn unpack(bytes: &[u8], seals: &BTreeMap<String, Seal>) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut plain = Vec::new();
    GzDecoder::new(bytes)
        .take(MAX_UNPACKED + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 > MAX_UNPACKED {
        return Err("protocol fixture expansion exceeds bound".into());
    }
    let mut archive = tar::Archive::new(plain.as_slice());
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for entry in archive.entries()?.raw(true) {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() || files.len() >= 319 {
            return Err("protocol fixture contains nonregular or excessive members".into());
        }
        let raw = entry.path_bytes();
        let name = std::str::from_utf8(&raw)?.to_owned();
        relative(&name)?;
        let seal = seals
            .get(&name)
            .ok_or("unexpected protocol fixture member")?;
        if files.contains_key(&name) || entry.size() != seal.bytes {
            return Err("duplicate or resized protocol fixture member".into());
        }
        total = total
            .checked_add(seal.bytes)
            .ok_or("fixture size overflow")?;
        if total > MAX_UNPACKED {
            return Err("protocol fixture data exceeds bound".into());
        }
        let mut data = Vec::new();
        entry.by_ref().take(seal.bytes + 1).read_to_end(&mut data)?;
        if data.len() as u64 != seal.bytes || sha(&data) != seal.sha256 {
            return Err("protocol fixture member hash changed".into());
        }
        files.insert(name, data);
    }
    if files.keys().ne(seals.keys()) {
        return Err("incomplete protocol fixture roster".into());
    }
    Ok(files)
}
/// Verify all packaged bytes without running any captured program or request.
pub fn verify_blob(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    if sha(LEDGER) != LEDGER_SHA256 || bytes.len() as u64 > MAX_PACKED {
        return Err("protocol fixture ledger or size changed".into());
    }
    let ledger: Ledger = serde_json::from_slice(LEDGER)?;
    if ledger.blob_sha256 != BLOB_SHA256
        || bytes.len() as u64 != ledger.blob_bytes
        || sha(bytes) != BLOB_SHA256
        || ledger.files.len() != 319
        || ledger.unpacked_bytes > MAX_UNPACKED
    {
        return Err("protocol fixture blob seal changed".into());
    }
    let files = unpack(bytes, &ledger.files)?;
    if files.values().map(|v| v.len() as u64).sum::<u64>() != ledger.unpacked_bytes {
        return Err("protocol fixture total size changed".into());
    }
    let manifest_bytes = &files["capture/capture-manifest.json"];
    if sha(manifest_bytes) != MANIFEST_SHA256 {
        return Err("protocol capture manifest changed".into());
    }
    let manifest: Value = serde_json::from_slice(manifest_bytes)?;
    if manifest["source_commit"] != SOURCE_COMMIT
        || manifest["source_builder_manifest_sha256"] != sha(&files["builder.json"])
    {
        return Err("protocol fixture provenance changed".into());
    }
    let seals = manifest["files_b64"]
        .as_object()
        .ok_or("missing capture seals")?;
    if seals.len() != 317 {
        return Err("protocol original capture roster changed".into());
    }
    let mut consumed = BTreeSet::new();
    for (encoded, seal) in seals {
        let decoded = STANDARD.decode(encoded)?;
        let name = std::str::from_utf8(&decoded)?;
        relative(name)?;
        let name = format!("capture/{name}");
        let data = files.get(&name).ok_or("missing original fixture member")?;
        if !consumed.insert(name)
            || seal["size"] != data.len() as u64
            || seal["sha256"] != sha(data)
        {
            return Err("protocol original member seal changed".into());
        }
    }
    Ok(files)
}
fn verify_sources(root: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let builder: Value = serde_json::from_slice(&files["builder.json"])?;
    let manifest: Value = serde_json::from_slice(&files["capture/capture-manifest.json"])?;
    let sources = builder["sources"]
        .as_object()
        .ok_or("missing C++ source pins")?;
    if sources.len() != 14 || builder["sources"] != manifest["source_builder_sources"] {
        return Err("protocol C++ source roster changed".into());
    }
    for (name, expected) in sources {
        let path = root.join(relative(name)?);
        if expected != &Value::String(sha(&read_regular(&path, MAX_PACKED)?)) {
            return Err(format!("protocol C++ source changed: {name}").into());
        }
    }
    Ok(())
}
/// Atomically stage a fresh complete fixture and environment receipts.
pub fn stage(root: &Path, output: &Path) -> Result<Value> {
    let parent = output.parent().ok_or("fixture output parent missing")?;
    if !root.is_absolute()
        || !output.is_absolute()
        || !fs::symlink_metadata(parent)?.file_type().is_dir()
    {
        return Err("fixture output must have a real existing absolute parent".into());
    }
    let output = parent
        .canonicalize()?
        .join(output.file_name().ok_or("output name missing")?);
    let growth = MAX_UNPACKED + 512 * 4096;
    disk_space::inspect(&output, growth)?.require()?;
    let blob = read_regular(
        &root.join(DIRECTORY).join("observations.tar.gz"),
        MAX_PACKED,
    )?;
    let files = verify_blob(&blob)?;
    verify_sources(root, &files)?;
    let staged = tempfile::Builder::new()
        .prefix(".protocol-reference-")
        .tempdir_in(parent)?;
    for (name, data) in &files {
        let destination = staged.path().join(relative(name)?);
        fs::create_dir_all(destination.parent().ok_or("fixture parent missing")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        file.write_all(data)?;
    }
    let environment = BTreeMap::from([
        (
            "PANDAD_PROTOCOL_ORIGINAL_CAPTURE",
            output
                .join("capture")
                .into_os_string()
                .into_string()
                .map_err(|_| "fixture path must be UTF-8")?,
        ),
        (
            "PANDAD_PROTOCOL_CAPTURE_MANIFEST_SHA256",
            MANIFEST_SHA256.to_owned(),
        ),
        (
            "PANDAD_PROTOCOL_CAPTURE_SOURCE_COMMIT",
            SOURCE_COMMIT.to_owned(),
        ),
        (
            "PANDAD_PROTOCOL_CAPTURE_BUILDER_MANIFEST",
            output
                .join("builder.json")
                .into_os_string()
                .into_string()
                .map_err(|_| "fixture path must be UTF-8")?,
        ),
    ]);
    let mut env = String::new();
    for (name, value) in &environment {
        if value.contains(['\n', '\r']) {
            return Err("fixture environment path contains a newline".into());
        }
        env.push_str(&format!("{name}={value}\n"));
    }
    fs::write(staged.path().join("environment.env"), env)?;
    let report = json!({"fixture_blob_sha256":BLOB_SHA256,"capture_manifest_sha256":MANIFEST_SHA256,
        "source_commit":SOURCE_COMMIT,"files":319,"original_files":317,"cpp_sources":14,
        "historical_programs_executed":false,"environment":environment});
    fs::write(
        staged.path().join("environment.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    verify_sources(root, &files)?;
    if read_regular(
        &root.join(DIRECTORY).join("observations.tar.gz"),
        MAX_PACKED,
    )? != blob
    {
        return Err("protocol fixture blob changed during staging".into());
    }
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged.path(),
        rustix::fs::CWD,
        &output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _published = staged.keep();
    Ok(report)
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut output = None;
    while let Some(option) = args.next() {
        let slot = match option.to_str() {
            Some("--root") => &mut root,
            Some("--output") => &mut output,
            _ => return Err("expected --root ROOT --output ABSENT_DIRECTORY".into()),
        };
        if slot.is_some() {
            return Err("duplicate fixture option".into());
        }
        *slot = Some(PathBuf::from(
            args.next().ok_or("fixture option needs a value")?,
        ));
    }
    let report = stage(
        &root.ok_or("--root required")?,
        &output.ok_or("--output required")?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    fn archive(names: &[(&str, tar::EntryType)]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut tar = tar::Builder::new(encoder);
        for (name, kind) in names {
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(*kind);
            header.set_mode(0o644);
            header.set_size(1);
            // Deliberately bypass Builder's safe path setter to exercise the
            // extractor itself, including traversal and special members.
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar.append(&header, b"x".as_slice()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }
    #[test]
    fn extraction_rejects_escape_links_duplicates_missing_members_and_bad_hash() {
        let seals = BTreeMap::from([(
            "safe".to_owned(),
            Seal {
                bytes: 1,
                sha256: sha(b"x"),
            },
        )]);
        for members in [
            vec![("../safe", tar::EntryType::Regular)],
            vec![("/safe", tar::EntryType::Regular)],
            vec![("a/./safe", tar::EntryType::Regular)],
            vec![("safe", tar::EntryType::Symlink)],
            vec![("safe", tar::EntryType::Link)],
            vec![("safe", tar::EntryType::Directory)],
            vec![("safe", tar::EntryType::Fifo)],
            vec![
                ("safe", tar::EntryType::Regular),
                ("safe", tar::EntryType::Regular),
            ],
            vec![("other", tar::EntryType::Regular)],
            vec![],
        ] {
            assert!(unpack(&archive(&members), &seals).is_err(), "{members:?}");
        }
        let mut wrong = seals;
        wrong.get_mut("safe").unwrap().sha256 = sha(b"y");
        assert!(unpack(&archive(&[("safe", tar::EntryType::Regular)]), &wrong).is_err());
    }
}
