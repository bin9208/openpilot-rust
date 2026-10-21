//! Bounded native materialization of externally sealed driving fixture data.
use crate::{disk_space, driving_daemon_reference::fixture::Fixture, seal::Result};
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};
const MAX_PACKED: u64 = 512 * 1024 * 1024;
const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;
pub const MANIFEST_SHA256: &str =
    "afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d";
pub const SOURCE_COMMIT: &str = "e25a14386582858dae25c6daca5ced3bfb4bacab";
/// Receipt values must come from the caller's independently pinned packaging receipt.
pub struct Config {
    pub archive: PathBuf,
    pub output: PathBuf,
    pub blob_sha256: String,
    pub blob_bytes: u64,
    pub compact_images: bool,
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
        return Err("unsafe driving fixture archive path".into());
    }
    Ok(Path::new(name))
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn generate_images(
    output: &Path,
    expected: &Value,
    actual: &mut BTreeMap<String, Value>,
) -> Result<Vec<Value>> {
    let mut generated = Vec::new();
    for width in [1344usize, 1928] {
        for mode in ["dual", "road", "wide"] {
            let warmup = format!("observations/{width}-{mode}/warmup.bin");
            let size: usize = expected[&warmup]["bytes"]
                .as_u64()
                .ok_or("missing frozen image size")?
                .try_into()?;
            if size == 0 || size > 8 * 1024 * 1024 || size % 8 != 0 {
                return Err("frozen image size outside generator bounds".into());
            }
            let count = if mode == "dual" { 12 } else { 4 };
            let mut random = crate::lagd_random::Generator::new(20260930);
            let mut buffer = vec![0u8; 65536];
            for frame in 0..=count {
                let names = if frame == 0 {
                    vec![format!("observations/{width}-{mode}/warmup.bin")]
                } else {
                    vec![
                        format!("observations/{width}-{mode}/images/main-{}.bin", frame - 1),
                        format!("observations/{width}-{mode}/images/extra-{}.bin", frame - 1),
                    ]
                };
                for (side, name) in names.iter().enumerate() {
                    if actual.contains_key(name) || expected[name]["bytes"] != size as u64 {
                        return Err("compact image omission/size roster changed".into());
                    }
                    let target = output.join(relative(name)?);
                    fs::create_dir_all(target.parent().ok_or("image parent missing")?)?;
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)?;
                    let mut digest = Sha256::new();
                    let mut left = size;
                    let mut main = if side == 1 && mode != "dual" {
                        Some(File::open(output.join(&names[0]))?)
                    } else {
                        None
                    };
                    while left > 0 {
                        let length = left.min(buffer.len());
                        let bytes = &mut buffer[..length];
                        if frame == 0 {
                            bytes.fill(0);
                        } else {
                            // NumPy full-range uint8 consumes little-endian PCG64
                            // uint32 halves. These array sizes are multiples of 8,
                            // so no buffered half remains across array calls.
                            for chunk in bytes.chunks_exact_mut(8) {
                                chunk.copy_from_slice(&random.raw().to_le_bytes());
                            }
                            // The original consumes the extra image even when a
                            // single-camera mode then substitutes the main image.
                            if let Some(main) = &mut main {
                                main.read_exact(bytes)?;
                            }
                        }
                        file.write_all(bytes)?;
                        digest.update(bytes);
                        left -= length;
                    }
                    let seal = json!({"bytes":size,"sha256":format!("{:x}",digest.finalize())});
                    if expected[name] != seal {
                        return Err(format!(
                            "generated image differs from original exact seal: {name}"
                        )
                        .into());
                    }
                    actual.insert(name.clone(), seal.clone());
                    generated
                        .push(json!({"name":name,"bytes":seal["bytes"],"sha256":seal["sha256"]}));
                }
            }
        }
    }
    if generated.len() != 86 {
        return Err("generated image roster changed".into());
    }
    Ok(generated)
}
fn extract(reader: impl Read, output: &Path, compact_images: bool) -> Result<Vec<Value>> {
    let bounded = GzDecoder::new(reader).take(MAX_UNPACKED + 1);
    let mut archive = tar::Archive::new(bounded);
    let mut actual = BTreeMap::new();
    let mut total = 0u64;
    for entry in archive.entries()?.raw(true) {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() || actual.len() >= 8001 {
            return Err("nonregular or excessive driving fixture members".into());
        }
        let raw = entry.path_bytes();
        let name = std::str::from_utf8(&raw)?.to_owned();
        let rel = relative(&name)?;
        if name != "fixture.json"
            && !["models", "catalog", "observations"]
                .iter()
                .any(|prefix| name.starts_with(&format!("{prefix}/")))
        {
            return Err("extraneous driving fixture member".into());
        }
        if matches!(rel.extension().and_then(|p| p.to_str()), Some("py" | "pyc"))
            || actual.contains_key(&name)
            || entry.size() > MAX_PACKED
        {
            return Err("duplicate, forbidden or oversized driving fixture member".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("fixture size overflow")?;
        if total > MAX_UNPACKED {
            return Err("driving fixture expansion exceeds bound".into());
        }
        let target = output.join(rel);
        fs::create_dir_all(target.parent().ok_or("fixture parent missing")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let mut digest = Sha256::new();
        let mut copied = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            digest.update(&buffer[..n]);
            file.write_all(&buffer[..n])?;
        }
        if copied != entry.size() {
            return Err("truncated driving fixture member".into());
        }
        actual.insert(
            name,
            json!({"bytes":copied,"sha256":format!("{:x}",digest.finalize())}),
        );
    }
    let mut remaining = archive.into_inner();
    std::io::copy(&mut remaining, &mut std::io::sink())?;
    if remaining.limit() == 0 {
        return Err("driving fixture gzip expansion exceeds bound".into());
    }
    let pin = actual
        .remove("fixture.json")
        .ok_or("missing fixture.json")?;
    if pin["sha256"] != MANIFEST_SHA256
        || pin["bytes"].as_u64().ok_or("manifest size missing")? > 2_000_000
    {
        return Err("driving fixture manifest pin changed".into());
    }
    let manifest: Value = serde_json::from_slice(&fs::read(output.join("fixture.json"))?)?;
    let generated = if compact_images {
        generate_images(output, &manifest["files"], &mut actual)?
    } else {
        Vec::new()
    };
    if manifest["source_commit"] != SOURCE_COMMIT
        || serde_json::to_value(actual)? != manifest["files"]
    {
        return Err("driving fixture complete roster changed".into());
    }
    Ok(generated)
}
pub fn stage(config: &Config) -> Result<Value> {
    if config.blob_bytes == 0
        || config.blob_bytes > MAX_PACKED
        || config.blob_sha256.len() != 64
        || !config.blob_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || !config.archive.is_absolute()
        || !config.output.is_absolute()
    {
        return Err("invalid driving archive receipt or paths".into());
    }
    for p in config
        .archive
        .ancestors()
        .skip(1)
        .chain(config.output.ancestors().skip(1))
    {
        if !fs::symlink_metadata(p)?.is_dir() {
            return Err("archive path parent is not a real directory".into());
        }
    }
    let parent = config.output.parent().ok_or("output parent missing")?;
    disk_space::inspect(parent, MAX_UNPACKED)?.require()?;
    let mut input = File::from(rustix::fs::open(
        &config.archive,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let before = input.metadata()?;
    if !before.is_file() || before.len() != config.blob_bytes {
        return Err("archive input not sealed regular file".into());
    }
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > config.blob_bytes {
            return Err("archive grew".into());
        }
        hash.update(&buffer[..n]);
    }
    if read != config.blob_bytes || format!("{:x}", hash.finalize()) != config.blob_sha256 {
        return Err("archive blob pin changed".into());
    }
    input.seek(SeekFrom::Start(0))?;
    let temp = tempfile::Builder::new()
        .prefix(".driving-fixture-")
        .tempdir_in(parent)?;
    let generated = extract(
        (&mut input).take(config.blob_bytes + 1),
        temp.path(),
        config.compact_images,
    )?;
    Fixture::open(temp.path(), MANIFEST_SHA256)?.verify()?;
    if identity(&before) != identity(&input.metadata()?)
        || identity(&before) != identity(&fs::symlink_metadata(&config.archive)?)
    {
        return Err("archive changed while staging".into());
    }
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temp.path(),
        rustix::fs::CWD,
        &config.output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _published = temp.keep();
    Fixture::open(&config.output, MANIFEST_SHA256)?.verify()?;
    Ok(
        json!({"output":config.output,"blob_sha256":config.blob_sha256,"blob_bytes":config.blob_bytes,"fixture_manifest_sha256":MANIFEST_SHA256,"source_commit":SOURCE_COMMIT,"historical_programs_executed":false,"compact_images":config.compact_images,"generated_images":generated}),
    )
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    let mut compact_images = false;
    while let Some(key) = args.next() {
        let key = key.into_string().map_err(|_| "non-UTF8 archive option")?;
        if key == "--compact-images" {
            if compact_images {
                return Err("duplicate compact image option".into());
            }
            compact_images = true;
            continue;
        }
        if !["--archive", "--output", "--blob-sha256", "--blob-bytes"].contains(&key.as_str())
            || values.contains_key(&key)
        {
            return Err("unknown or duplicate archive option".into());
        }
        values.insert(key, args.next().ok_or("archive option needs value")?);
    }
    let mut take = |key: &str| values.remove(key).ok_or_else(|| format!("{key} required"));
    let archive = PathBuf::from(take("--archive")?);
    let output = PathBuf::from(take("--output")?);
    let blob_sha256 = take("--blob-sha256")?
        .into_string()
        .map_err(|_| "invalid blob hash")?;
    let blob_bytes = take("--blob-bytes")?
        .into_string()
        .map_err(|_| "invalid blob size")?
        .parse()?;
    println!(
        "{}",
        stage(&Config {
            archive,
            output,
            blob_sha256,
            blob_bytes,
            compact_images,
        })?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    fn packed(members: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (name, kind, data) in members {
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(*kind);
            header.set_mode(0o644);
            header.set_size(data.len() as u64);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            archive.append(&header, *data).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }
    #[test]
    fn rejects_escape_special_duplicate_extraneous_missing_and_changed_manifest() {
        for members in [
            vec![("../escape", tar::EntryType::Regular, b"x".as_slice())],
            vec![("/escape", tar::EntryType::Regular, b"x".as_slice())],
            vec![("models/../escape", tar::EntryType::Regular, b"x".as_slice())],
            vec![("models/link", tar::EntryType::Symlink, b"x".as_slice())],
            vec![("models/link", tar::EntryType::Link, b"x".as_slice())],
            vec![("models/fifo", tar::EntryType::Fifo, b"x".as_slice())],
            vec![("models/dir", tar::EntryType::Directory, b"".as_slice())],
            vec![("unexpected", tar::EntryType::Regular, b"x".as_slice())],
            vec![("models/tool.py", tar::EntryType::Regular, b"x".as_slice())],
            vec![
                ("models/a", tar::EntryType::Regular, b"x".as_slice()),
                ("models/a", tar::EntryType::Regular, b"x".as_slice()),
            ],
            vec![("models/a", tar::EntryType::Regular, b"x".as_slice())],
            vec![("fixture.json", tar::EntryType::Regular, b"{}".as_slice())],
            vec![],
        ] {
            let temp = tempfile::tempdir().unwrap();
            assert!(
                extract(packed(&members).as_slice(), temp.path(), false).is_err(),
                "{members:?}"
            );
            assert!(!temp.path().parent().unwrap().join("escape").exists());
        }
    }
    #[test]
    fn compact_generation_rejects_unsealed_sizes_and_included_image_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut actual = BTreeMap::new();
        assert!(generate_images(temp.path(), &json!({}), &mut actual).is_err());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        actual.insert(
            "observations/1344-dual/warmup.bin".to_owned(),
            json!({"bytes":0,"sha256":"wrong"}),
        );
        assert!(generate_images(temp.path(), &json!({}), &mut actual).is_err());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
