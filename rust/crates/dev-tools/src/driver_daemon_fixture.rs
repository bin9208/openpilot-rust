//! Durable independent driver observations. Historical PKLs are opaque data.
use crate::{disk_space, driver_daemon_check::OriginalCapture, seal::Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub const DIRECTORY: &str = "rust/crates/dev-tools/tests/fixtures/driver-daemon-original";
const MAX_PACKED: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 192 * 1024 * 1024;
const LEDGER: &[u8] = include_bytes!("../tests/fixtures/driver-daemon-original/fixture.json");
pub const COLLECTOR_MANIFEST: &str =
    "57a3a05c0a0fa5b3318fc3fb1e3c07e9a9838b3379c3cb2e50e96974ed7bfd4a";
pub const NO_COLLECTOR_MANIFEST: &str =
    "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380";
pub const CURRENT_COLLECTOR_MANIFEST: &str =
    "532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795";
const LEDGER_SHA: &str = "c55233339671d092fc7d67485e59c639b6fab1ffb03b1c4ece5138e4e9547a33";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn pin(bytes: &[u8]) -> Value {
    json!({"bytes":bytes.len(),"sha256":sha(bytes)})
}
fn relative(name: &str) -> Result<&Path> {
    if name.is_empty()
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || !Path::new(name)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
        || name.ends_with(".py")
        || name.ends_with(".pyc")
    {
        return Err("unsafe driver fixture member".into());
    }
    Ok(Path::new(name))
}
fn real_parent(path: &Path) -> Result<()> {
    for parent in path.ancestors().skip(1) {
        let m = fs::symlink_metadata(parent)?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("driver fixture parent alias".into());
        }
    }
    Ok(())
}
fn regular(path: &Path, limit: u64) -> Result<Vec<u8>> {
    real_parent(path)?;
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let file = File::from(fd);
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit {
        return Err("driver fixture input outside bound".into());
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
        return Err("driver fixture input changed".into());
    }
    Ok(bytes)
}
fn unpack(blob: &[u8], expected: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Vec<u8>>> {
    let decoder = xz2::stream::Stream::new_stream_decoder(256 * 1024 * 1024, 0)?;
    // The limit applies before tar parsing; the complete expanded dataset also
    // has an independent bound after omitted images are regenerated.
    let mut plain = Vec::new();
    let mut decoder = xz2::bufread::XzDecoder::new_stream(blob, decoder);
    (&mut decoder).take(MAX_TOTAL + 1).read_to_end(&mut plain)?;
    if !decoder.into_inner().is_empty() {
        return Err("trailing compressed bytes or second driver archive stream".into());
    }
    if plain.len() as u64 > MAX_TOTAL {
        return Err("driver archive expansion exceeds bound".into());
    }
    let mut archive = tar::Archive::new(plain.as_slice());
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for entry in archive.entries()?.raw(true) {
        let entry = entry?;
        if !entry.header().entry_type().is_file() || files.len() >= 128 {
            return Err("driver archive special or excessive member".into());
        }
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        relative(&name)?;
        let expected = expected
            .get(&name)
            .ok_or("unexpected driver archive member")?;
        if files.contains_key(&name) || expected["bytes"] != entry.size() {
            return Err("duplicate or resized driver archive member".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("driver archive size overflow")?;
        if total > MAX_TOTAL {
            return Err("driver archive payload exceeds bound".into());
        }
        let mut bytes = Vec::new();
        entry.take(MAX_TOTAL + 1).read_to_end(&mut bytes)?;
        if pin(&bytes) != *expected {
            return Err("driver archive member seal changed".into());
        }
        files.insert(name, bytes);
    }
    if archive.into_inner().iter().any(|&byte| byte != 0) {
        return Err("nonzero content after driver tar terminator".into());
    }
    if files.len() != expected.len() {
        return Err("driver archive member missing".into());
    }
    Ok(files)
}
pub fn verify_blob(blob: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    if sha(LEDGER) != LEDGER_SHA || blob.len() as u64 > MAX_PACKED {
        return Err("driver fixture ledger or size changed".into());
    }
    let ledger: Value = serde_json::from_slice(LEDGER)?;
    if ledger["version"] != 1
        || ledger["blob"] != pin(blob)
        || ledger["regenerated_images"] != 42
        || ledger["collector_data_ready"] != true
        || ledger["native_retirement_accepted"] != false
        || ledger["historical_collector_execution_ready"] != false
    {
        return Err("driver fixture identity changed".into());
    }
    let expected: BTreeMap<String, Value> = serde_json::from_value(ledger["packed_files"].clone())?;
    let files = unpack(blob, &expected)?;
    for (kind, digest) in [
        ("collector", COLLECTOR_MANIFEST),
        ("no-collector", NO_COLLECTOR_MANIFEST),
        ("collector-current", CURRENT_COLLECTOR_MANIFEST),
    ] {
        let bytes = files
            .get(&format!("{kind}/observations/reference-manifest.json"))
            .ok_or("driver original manifest missing")?;
        if sha(bytes) != digest {
            return Err("driver independent original manifest changed".into());
        }
        let manifest: Value = serde_json::from_slice(bytes)?;
        let dataset = &ledger["datasets"][kind];
        if manifest["source_commit"] != dataset["source_commit"]
            || manifest["original_models"] != dataset["models"]
            || dataset["original_files"] != 43
            || dataset["source_expected_packets"] != 14
        {
            return Err("driver dataset provenance changed".into());
        }
        let original = manifest["files"]
            .as_object()
            .ok_or("driver original roster missing")?;
        if original.len() != 43 {
            return Err("driver original roster changed".into());
        }
        for (name, seal) in original {
            relative(name)?;
            let full = format!("{kind}/observations/{name}");
            if ledger["files"][&full] != *seal {
                return Err("driver ledger diverges from original seal".into());
            }
        }
        if kind == "no-collector" {
            for (name, seal) in dataset["models"]
                .as_object()
                .ok_or("driver model pins missing")?
            {
                let full = format!("{kind}/models/{name}");
                if files.get(&full).map(|b| pin(b)).as_ref() != Some(seal) {
                    return Err("driver original PKL missing or changed".into());
                }
            }
        } else if kind == "collector-current" {
            if dataset["models_available"] != true
                || dataset["models"] != ledger["datasets"]["no-collector"]["models"]
            {
                return Err("current collector exact shared model pins changed".into());
            }
        } else if dataset["models_available"] != false
            || files.keys().any(|n| n.starts_with("collector/models/"))
        {
            return Err("unverified historical collector model substitution".into());
        }
    }
    Ok(files)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("driver member parent missing")?)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}
fn generate_images(output: &Path, ledger: &Value) -> Result<usize> {
    let mut generated = 0;
    for kind in ["collector", "no-collector", "collector-current"] {
        for width in [1344, 1928] {
            for raw in [true, false] {
                let directory = format!("{kind}/observations/{width}-raw-{}", u8::from(raw));
                let warmup = format!("{directory}/warmup.bin");
                let length = ledger["files"][&warmup]["bytes"]
                    .as_u64()
                    .ok_or("driver warmup size missing")?;
                let expected = if width == 1344 { 2428928 } else { 4804608 };
                if length != expected {
                    return Err("driver frozen image layout changed".into());
                }
                let mut rng = crate::lagd_random::Generator::new(20260930);
                for frame in 0..=if raw { 4 } else { 1 } {
                    let name = if frame == 0 {
                        warmup.clone()
                    } else {
                        format!("{directory}/frame-{}.bin", frame - 1)
                    };
                    if ledger["packed_files"].get(&name).is_some() {
                        return Err("driver compact image unexpectedly packed".into());
                    }
                    let mut bytes = vec![0; length as usize];
                    if frame != 0 {
                        for chunk in bytes.chunks_exact_mut(8) {
                            chunk.copy_from_slice(&rng.raw().to_le_bytes())
                        }
                    }
                    if ledger["files"][&name] != pin(&bytes) {
                        return Err(format!(
                            "driver generated image differs from exact original seal: {name}"
                        )
                        .into());
                    }
                    write_new(&output.join(relative(&name)?), &bytes)?;
                    generated += 1;
                }
            }
        }
    }
    if generated != 42 {
        return Err("driver compact generation roster changed".into());
    }
    Ok(generated)
}
pub fn stage(root: &Path, output: &Path) -> Result<Value> {
    if !output.is_absolute() {
        return Err("driver fixture output must be absolute".into());
    }
    real_parent(output)?;
    match fs::symlink_metadata(output) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err("driver fixture output already exists or is inaccessible".into()),
    }
    let archive = root.join(DIRECTORY).join("observations.tar.xz");
    let blob = regular(&archive, MAX_PACKED)?;
    let files = verify_blob(&blob)?;
    let ledger: Value = serde_json::from_slice(LEDGER)?;
    let expected: BTreeMap<String, Value> = serde_json::from_value(ledger["files"].clone())?;
    let total = expected
        .values()
        .try_fold(0u64, |sum, p| {
            sum.checked_add(p["bytes"].as_u64()?)
                .filter(|n| *n <= MAX_TOTAL)
        })
        .ok_or("driver complete fixture exceeds bound")?;
    disk_space::inspect(output, total)?.require()?;
    let temp = tempfile::Builder::new()
        .prefix(".driver-observations-")
        .tempdir_in(output.parent().ok_or("driver fixture parent missing")?)?;
    for (name, bytes) in &files {
        write_new(&temp.path().join(relative(name)?), bytes)?
    }
    let regenerated = generate_images(temp.path(), &ledger)?;
    for (name, seal) in &expected {
        if pin(&regular(&temp.path().join(relative(name)?), MAX_TOTAL)?) != *seal {
            return Err("staged driver file seal changed".into());
        }
    }
    for (kind, digest) in [
        ("collector", COLLECTOR_MANIFEST),
        ("no-collector", NO_COLLECTOR_MANIFEST),
        ("collector-current", CURRENT_COLLECTOR_MANIFEST),
    ] {
        OriginalCapture::open(
            &temp.path().join(kind).join("observations"),
            digest,
            ledger["datasets"][kind]["source_commit"]
                .as_str()
                .ok_or("driver dataset source missing")?,
        )?
        .verify()?;
    }
    let report = json!({"fixture_blob":ledger["blob"],"datasets":ledger["datasets"],"regenerated_images":regenerated,"historical_programs_executed":false,"collector_data_ready":true,"native_retirement_accepted":false,"historical_collector_execution_ready":false,"historical_collector_models":null,"collector_models":output.join("no-collector/models"),"no_collector_models":output.join("no-collector/models"),"historical_collector_original_directory":output.join("collector/observations"),"collector_original_directory":output.join("collector-current/observations"),"no_collector_original_directory":output.join("no-collector/observations")});
    write_new(
        &temp.path().join("staging-receipt.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    if regular(&archive, MAX_PACKED)? != blob {
        return Err("driver source archive changed during staging".into());
    }
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temp.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _ = temp.keep();
    Ok(report)
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if !["--root", "--output"].iter().any(|n| key == *n) || values.contains_key(&key) {
            return Err("unknown or duplicate driver fixture option".into());
        }
        values.insert(key, args.next().ok_or("driver fixture option needs value")?);
    }
    let root = PathBuf::from(
        values
            .remove(&std::ffi::OsString::from("--root"))
            .ok_or("--root required")?,
    );
    let output = PathBuf::from(
        values
            .remove(&std::ffi::OsString::from("--output"))
            .ok_or("--output required")?,
    );
    println!("{}", stage(&root, &output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive(members: &[(&str, tar::EntryType)]) -> Vec<u8> {
        let mut archive = tar::Builder::new(Vec::new());
        for (name, kind) in members {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_entry_type(*kind);
            header.set_size(1);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            archive.append(&header, b"x".as_slice()).unwrap();
        }
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
        encoder.write_all(&archive.into_inner().unwrap()).unwrap();
        encoder.finish().unwrap()
    }
    #[test]
    fn archive_rejects_compressed_and_tar_trailing_payloads() {
        let seals = BTreeMap::from([("safe".to_owned(), pin(b"x"))]);
        let valid = archive(&[("safe", tar::EntryType::Regular)]);
        let mut trailing = valid.clone();
        trailing.push(1);
        assert!(unpack(&trailing, &seals).is_err());
        let mut second = valid.clone();
        second.extend_from_slice(&valid);
        assert!(unpack(&second, &seals).is_err());
        let mut plain = Vec::new();
        xz2::read::XzDecoder::new(valid.as_slice())
            .read_to_end(&mut plain)
            .unwrap();
        plain.push(1);
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
        encoder.write_all(&plain).unwrap();
        assert!(unpack(&encoder.finish().unwrap(), &seals).is_err());
    }
    #[test]
    fn archive_admission_rejects_escape_special_duplicate_extra_missing_and_changed_data() {
        let seals = BTreeMap::from([("safe".to_owned(), pin(b"x"))]);
        assert_eq!(
            unpack(&archive(&[("safe", tar::EntryType::Regular)]), &seals).unwrap()["safe"],
            b"x"
        );
        for members in [
            vec![("../escape", tar::EntryType::Regular)],
            vec![("/escape", tar::EntryType::Regular)],
            vec![("safe", tar::EntryType::Symlink)],
            vec![("safe", tar::EntryType::Link)],
            vec![("safe", tar::EntryType::Fifo)],
            vec![("safe", tar::EntryType::Directory)],
            vec![
                ("safe", tar::EntryType::Regular),
                ("safe", tar::EntryType::Regular),
            ],
            vec![("extra", tar::EntryType::Regular)],
            vec![],
        ] {
            assert!(unpack(&archive(&members), &seals).is_err());
        }
        let wrong = BTreeMap::from([("safe".to_owned(), pin(b"y"))]);
        assert!(unpack(&archive(&[("safe", tar::EntryType::Regular)]), &wrong).is_err());
    }
}
