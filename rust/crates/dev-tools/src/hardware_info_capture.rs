//! Admission and final revalidation of the immutable original HardwareInfo corpus.
use crate::seal::Result;
use flate2::bufread::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const MANIFEST_SHA: &str = "2a0296cd351c530457f7fc9a3219176f0e533eb30a5da325a5bc1f0307a545af";
const RETIRED: [&str; 3] = [
    "rust/tools/check_hardware_info.py",
    "rust/tools/hardware_info_cases.py",
    "rust/tools/hardware_info_reference.py",
];
const MEMBER_LIMIT: u64 = 128 * 1024 * 1024;
type Identity = (u64, u64, u64, i64, i64, i64, i64);
fn identity(m: &fs::Metadata) -> Identity {
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
fn safe(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("capture path must be absolute without dot components".into());
    }
    for parent in path.ancestors() {
        if fs::symlink_metadata(parent)?.file_type().is_symlink() {
            return Err("capture path has a symlink component".into());
        }
    }
    Ok(())
}
fn relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    if path.as_os_str().is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("invalid capture member path".into());
    }
    Ok(path)
}
fn digest(reader: &mut impl Read, limit: u64) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut count = 0;
    let mut buffer = [0; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > limit {
            return Err("capture read exceeds bound".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok((count, format!("{:x}", hash.finalize())))
}
struct Held {
    path: PathBuf,
    file: File,
    id: Identity,
    sha: String,
}
impl Held {
    fn open(path: &Path, limit: u64) -> Result<Self> {
        safe(path)?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err("invalid capture input type or size".into());
        }
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let id = identity(&metadata);
        if identity(&file.metadata()?) != id {
            return Err("capture input replaced while opening".into());
        }
        let (_, sha) = digest(&mut file, limit)?;
        let held = Self {
            path: path.into(),
            file,
            id,
            sha,
        };
        held.verify()?;
        Ok(held)
    }
    fn verify(&self) -> Result<()> {
        safe(&self.path)?;
        if identity(&fs::symlink_metadata(&self.path)?) != self.id
            || identity(&self.file.metadata()?) != self.id
        {
            return Err("held capture input identity changed".into());
        }
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let (bytes, sha) = digest(&mut file, self.id.2)?;
        if bytes != self.id.2 || sha != self.sha || identity(&self.file.metadata()?) != self.id {
            return Err("held capture input bytes changed".into());
        }
        Ok(())
    }
    fn report(&self) -> Value {
        json!({"path":self.path,"identity":self.id,"bytes":self.id.2,"sha256":self.sha})
    }
}
#[derive(PartialEq)]
struct Snapshot {
    id: Identity,
    compressed: (u64, String),
    expanded: (u64, String),
}
fn snapshot(path: &Path, expected: &Value) -> Result<Snapshot> {
    let held = Held::open(path, 512 * 1024)?;
    let compressed = (held.id.2, held.sha.clone());
    let expanded = if path.extension().is_some_and(|extension| extension == "gz") {
        let mut file = held.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut decoder = GzDecoder::new(BufReader::new(file));
        let expanded = digest(&mut decoder, MEMBER_LIMIT)?;
        let mut remainder = decoder.into_inner();
        let mut tail = [0];
        if remainder.read(&mut tail)? != 0 {
            return Err("capture gzip has trailing bytes or another stream".into());
        }
        expanded
    } else {
        compressed.clone()
    };
    if expected["bytes"].as_u64() != Some(compressed.0)
        || expected["sha256"].as_str() != Some(&compressed.1)
        || expected["uncompressed_bytes"].as_u64() != Some(expanded.0)
        || expected["uncompressed_sha256"].as_str() != Some(&expanded.1)
    {
        return Err("capture compressed or expanded seal differs".into());
    }
    held.verify()?;
    Ok(Snapshot {
        id: held.id,
        compressed,
        expanded,
    })
}
fn absent(path: &Path) -> Result<()> {
    safe(path.parent().ok_or("retired source has no parent")?)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("retired HardwareInfo source must be actually absent".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }
    fn entry(compressed: &[u8], expanded: &[u8]) -> Value {
        json!({"bytes":compressed.len(),"sha256":format!("{:x}",Sha256::digest(compressed)),
            "uncompressed_bytes":expanded.len(),"uncompressed_sha256":format!("{:x}",Sha256::digest(expanded))})
    }
    #[test]
    fn gzip_requires_full_checksum_and_single_stream() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("member.gz");
        let bytes = gzip(b"independent source data");
        fs::write(&path, &bytes).unwrap();
        assert!(snapshot(&path, &entry(&bytes, b"independent source data")).is_ok());
        let mut extra = bytes.clone();
        extra.extend(gzip(b"unadmitted second stream"));
        fs::write(&path, &extra).unwrap();
        assert!(snapshot(&path, &entry(&extra, b"independent source data")).is_err());
        fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert!(snapshot(
            &path,
            &entry(&bytes[..bytes.len() - 1], b"independent source data")
        )
        .is_err());
        let plain = directory.path().join("receipt.json");
        fs::write(&plain, b"{\"source\":true}").unwrap();
        assert!(snapshot(&plain, &entry(b"{\"source\":true}", b"{\"source\":true}")).is_ok());
    }
    #[test]
    fn containment_and_absence_reject_symlink_aliases() {
        for name in ["../outside", "/absolute", "case/../outside", ""] {
            assert!(relative(name).is_err());
        }
        let directory = tempfile::tempdir().unwrap();
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(directory.path(), &alias).unwrap();
        assert!(absent(&alias.join("missing.py")).is_err());
        assert!(absent(&alias).is_err());
    }
}
fn inventory(root: &Path, path: &Path, output: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            inventory(root, &path, output)?;
        } else if metadata.is_file() {
            output.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF8 capture member")?
                    .into(),
            );
        } else {
            return Err("capture contains symlink or special member".into());
        }
    }
    Ok(())
}
pub struct Receipt {
    root: PathBuf,
    capture: PathBuf,
    retired: bool,
    held: Vec<Held>,
    missing: Vec<String>,
    manifest: Value,
    files: BTreeMap<String, Snapshot>,
}
impl Receipt {
    pub fn verify(&self) -> Result<()> {
        for held in &self.held {
            held.verify()?;
        }
        for name in &self.missing {
            absent(&self.root.join(name))?;
        }
        safe(&self.capture)?;
        let mut names = BTreeSet::new();
        inventory(&self.capture, &self.capture, &mut names)?;
        names.remove("capture-manifest.json");
        names.remove("README.md");
        if names != self.files.keys().cloned().collect() {
            return Err("capture member roster changed".into());
        }
        for (name, before) in &self.files {
            if snapshot(
                &self.capture.join(relative(name)?),
                &self.manifest["files"][name],
            )? != *before
            {
                return Err("capture member identity changed".into());
            }
        }
        for held in &self.held {
            held.verify()?;
        }
        Ok(())
    }
    pub fn report(&self) -> Value {
        json!({"manifest_sha256":MANIFEST_SHA,"require_retired":self.retired,
            "held_inputs":self.held.iter().map(Held::report).collect::<Vec<_>>(),"absent_sources":self.missing,
            "fixture_files":self.files.iter().map(|(name,s)| (name,json!({"identity":s.id,
                "bytes":s.compressed.0,"sha256":s.compressed.1,"uncompressed_bytes":s.expanded.0,
                "uncompressed_sha256":s.expanded.1}))).collect::<BTreeMap<_,_>>()})
    }
}
pub fn validate(
    root: &Path,
    capture: &Path,
    plans: &Path,
    require_retired: bool,
    binary: &Path,
) -> Result<Receipt> {
    safe(root)?;
    safe(capture)?;
    safe(plans)?;
    if plans != capture.join("plans.json.gz") {
        return Err("plans must be the captured plans.json.gz".into());
    }
    let manifest_pin = Held::open(&capture.join("capture-manifest.json"), 4 * 1024 * 1024)?;
    if manifest_pin.sha != MANIFEST_SHA {
        return Err("unrecognized HardwareInfo manifest".into());
    }
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_pin.path)?)?;
    if manifest["schema"] != 1
        || manifest["independent_expected_data"] != true
        || manifest["scenario_count"] != 136
        || manifest["comparisons"] != 3110
        || manifest["datagram_requests_per_implementation"] != 70
    {
        return Err("HardwareInfo manifest contract differs".into());
    }
    let scenarios = manifest["ordered_scenarios"]
        .as_array()
        .ok_or("missing scenario roster")?;
    if scenarios.len() != 136
        || scenarios
            .iter()
            .any(|s| s["source_invocation"]["exit_code"] != 0)
    {
        return Err("original scenario completion receipt missing".into());
    }
    let mut native = Held::open(binary, 512 * 1024 * 1024)?;
    native.file.seek(SeekFrom::Start(0))?;
    let mut elf = [0; 20];
    native.file.read_exact(&mut elf)?;
    let machine = if cfg!(target_arch = "x86_64") {
        62
    } else if cfg!(target_arch = "aarch64") {
        183
    } else {
        return Err("unsupported native architecture".into());
    };
    if &elf[..4] != b"\x7fELF"
        || elf[4] != 2
        || elf[5] != 1
        || elf[6] != 1
        || !matches!(u16::from_le_bytes([elf[16], elf[17]]), 2 | 3)
        || u16::from_le_bytes([elf[18], elf[19]]) != machine
        || fs::metadata(binary)?.mode() & 0o111 == 0
    {
        return Err("binary is not an executable current-architecture ELF".into());
    }
    let mut held = vec![manifest_pin, native];
    let sources = manifest["source_sha256"]
        .as_object()
        .ok_or("missing source pins")?;
    if sources.len() != 30 {
        return Err("source roster differs".into());
    }
    let mut missing = Vec::new();
    for (name, expected) in sources {
        let path = root.join(relative(name)?);
        if RETIRED.contains(&name.as_str())
            && (require_retired
                || fs::symlink_metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound))
        {
            absent(&path)?;
            missing.push(name.clone());
        } else {
            let source = Held::open(&path, MEMBER_LIMIT)?;
            if expected.as_str() != Some(&source.sha) {
                return Err(format!("HardwareInfo source changed: {name}").into());
            }
            held.push(source);
        }
    }
    let entries = manifest["files"]
        .as_object()
        .ok_or("missing capture files")?;
    if entries.len() != 974 {
        return Err("capture file count differs".into());
    }
    let mut files = BTreeMap::new();
    let (mut compressed, mut expanded) = (0_u64, 0_u64);
    for (name, expected) in entries {
        let entry = snapshot(&capture.join(relative(name)?), expected)?;
        compressed += entry.compressed.0;
        expanded += entry.expanded.0;
        if compressed > 512 * 1024 || expanded > 256 * 1024 * 1024 {
            return Err("capture total exceeds admission budget".into());
        }
        files.insert(name.clone(), entry);
    }
    let receipt = Receipt {
        root: root.into(),
        capture: capture.into(),
        retired: require_retired,
        held,
        missing,
        manifest,
        files,
    };
    receipt.verify()?;
    Ok(receipt)
}
