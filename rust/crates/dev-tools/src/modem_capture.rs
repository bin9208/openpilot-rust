//! Independent original-source outputs and source-closure admission for modem QA.
use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::ffi::OsStrExt,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/modem-original/capture-manifest.json");
const MANIFEST_SHA256: &str = "ef03b3508abb966f5522959d05ccce9dd9d6e5eee0b7c8880dc111c8d339177c";
const PROCESS_BOUNDARY_SHA256: &str =
    "9b7d0fa7e3d8c79d6fe83519afcba2530a5f284f0c8c2fa31af960da3102c869";
const MAX_SOURCE: u64 = 32 * 1024 * 1024;
const MAX_EXPECTATION: u64 = 8 * 1024 * 1024;
const MAX_EXECUTABLE: u64 = 512 * 1024 * 1024;
const SOURCE_PINS: [(&str, &str); 6] = [
    (
        "openpilot/system/hardware/tici/modem.py",
        "c193b3f5c669dab5e488ee982cbe565539a0410193b4302d524f68c157d47a13",
    ),
    (
        "rust/tools/check_modem.py",
        "6b7df54af0d948f9569f6e933f70a63fb30c696b6e7ef47d5ddbfe960e19a922",
    ),
    (
        "rust/tools/modem_source.py",
        "72c75ec2185003a861e1512edf06921352bb3ff02f5b27d3e1a5a743a3964060",
    ),
    (
        "rust/tools/modem_fixture.py",
        "07ab85b2f6dcb66750a557c7b9ed4dae32503c8b2f482d2a28e984a65c63d22a",
    ),
    (
        "rust/tools/modem_fixture_command.py",
        "c99e412facaf74f6830db1a50d32cf16ef773da69131ffacb07b69571d8cbb52",
    ),
    (
        "rust/tools/check_modem_process_boundary.py",
        "2a3cd0907ddc175a5c753490b11784a458d904065236c520dffd949548cd2abd",
    ),
];
const CASES: [(&str, &str); 13] = [
    (
        "normal",
        "bd0c921972091ff5ec94fb436b04429e782aa37004c8a3f924739d4173b6b8ea",
    ),
    (
        "at_errors",
        "80c374086feeebeb62a82c13e369ec583d4934a80392068c15def573c9749283",
    ),
    (
        "lpa_lock",
        "0ab6bb5277765629e017484a66ca15faaab2a96cd051559229ced19af7b4a50f",
    ),
    (
        "echo_retry",
        "6630fe201992731fac43be9d452bc66f82a44136bb9b88b319ae6cf1c3b17827",
    ),
    (
        "identity_retry",
        "4a13060f36d6011862de9804c36f1e39960b82cad6815417091f6bbe50e3dac0",
    ),
    (
        "absent_sim",
        "559cedf27ece6446e8dea25cdae16a97924d951ee7784ccea825165053a0f157",
    ),
    (
        "roaming_blocked",
        "9db4720ae7945e3d7ac277620665b6b18bc1b1f86c9833ab351810b1f8396f07",
    ),
    (
        "poll_errors",
        "4ba104a98ad768d2169f5dcba0907822cfc9daf6a7baf5ffa358a731e535e302",
    ),
    (
        "retry_limit",
        "993fad20692a9fc7f1b2c8f4b95b544e19fea709c848d93c51e9849e6958dfab",
    ),
    (
        "route_failure_retry",
        "39482d15c1d5aaa0b76e696c6ca2f2c0ce64249ef5e8a26dea838b6b9838ff05",
    ),
    (
        "route_failure",
        "7c6ffea111a7ea91f09e93fc5970d6ad3c5422ff59f7f632764584ef1bd0bcb6",
    ),
    (
        "invalid_route",
        "97e86500e4b5f75b7d198b7f6290f5018fce7afbbf3935a116d1783677b83c79",
    ),
    (
        "dns_failure",
        "1d249da4b410c1276462d9567cd1614198f81f498c553164b94f4fe0f8d903a4",
    ),
];

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn safe_path(root: &Path, name: &str) -> Result<PathBuf> {
    let relative = Path::new(name);
    require(
        !name.is_empty()
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        format!("unsafe modem capture path: {name}"),
    )?;
    Ok(root.join(relative))
}

fn digest_file(file: &File, expected_len: u64, maximum: u64) -> Result<String> {
    require(
        expected_len <= maximum,
        "modem capture exceeds its read bound",
    )?;
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 32 * 1024];
    let mut total = 0u64;
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("modem capture length overflow")?;
        require(total <= maximum, "modem capture grew beyond its bound")?;
        hash.update(&bytes[..count]);
    }
    require(
        total == expected_len,
        "modem capture length changed while hashing",
    )?;
    Ok(format!("{:x}", hash.finalize()))
}

struct Held {
    name: String,
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    len: u64,
    mode: u32,
    mtime: (i64, i64),
    ctime: (i64, i64),
    sha256: String,
    maximum: u64,
}

impl Held {
    fn open(path: &Path, name: String, maximum: u64) -> Result<Self> {
        for parent in path.ancestors().skip(1) {
            let metadata = fs::symlink_metadata(parent)?;
            require(
                metadata.file_type().is_dir(),
                format!(
                    "modem input parent is not a real directory: {}",
                    parent.display()
                ),
            )?;
        }
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        require(
            metadata.is_file() && metadata.len() <= maximum,
            format!("modem input is nonregular or oversized: {name}"),
        )?;
        let mut held = Self {
            name,
            path: path.to_path_buf(),
            file,
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            mode: metadata.mode(),
            mtime: (metadata.mtime(), metadata.mtime_nsec()),
            ctime: (metadata.ctime(), metadata.ctime_nsec()),
            sha256: String::new(),
            maximum,
        };
        let before = held.identity();
        held.sha256 = digest_file(&held.file, held.len, held.maximum)?;
        require(
            held.identity() == before,
            format!("modem input changed while hashing: {}", held.name),
        )?;
        Ok(held)
    }

    fn identity(&self) -> (u64, u64, u64, u32, (i64, i64), (i64, i64)) {
        (
            self.dev, self.ino, self.len, self.mode, self.mtime, self.ctime,
        )
    }

    fn verify(&self) -> Result<()> {
        let metadata = self.file.metadata()?;
        let current = (
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mode(),
            (metadata.mtime(), metadata.mtime_nsec()),
            (metadata.ctime(), metadata.ctime_nsec()),
        );
        require(
            metadata.is_file() && current == self.identity(),
            format!("held modem input identity changed: {}", self.name),
        )?;
        require(
            digest_file(&self.file, self.len, self.maximum)? == self.sha256,
            format!("held modem input content changed: {}", self.name),
        )?;
        let path = Held::open(&self.path, self.name.clone(), self.maximum)?;
        require(
            path.identity() == self.identity() && path.sha256 == self.sha256,
            format!("modem input path was replaced: {}", self.name),
        )
    }

    fn report(&self) -> Value {
        json!({"path": self.path.to_string_lossy(), "path_bytes_hex": hex(self.path.as_os_str().as_bytes()),
          "bytes": self.len, "sha256": self.sha256,
          "device": self.dev, "inode": self.ino, "mode": self.mode,
          "mtime": self.mtime, "ctime": self.ctime})
    }

    fn executable(path: &Path, name: &str) -> Result<Self> {
        let held = Self::open(path, name.to_owned(), MAX_EXECUTABLE)?;
        require(
            held.mode & 0o111 != 0,
            format!("modem executable lacks execute permission: {name}"),
        )?;
        let mut file = held.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut elf = [0u8; 20];
        file.read_exact(&mut elf)?;
        let machine = u16::from_le_bytes([elf[18], elf[19]]);
        let current_machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported modem QA host architecture".into()),
        };
        require(
            &elf[..4] == b"\x7fELF"
                && elf[4] == 2
                && elf[5] == 1
                && elf[6] == 1
                && matches!(u16::from_le_bytes([elf[16], elf[17]]), 2 | 3)
                && machine == current_machine,
            format!("modem executable is not a current-host ELF: {name}"),
        )?;
        held.verify()?;
        Ok(held)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub struct Receipt {
    root: PathBuf,
    fixture: PathBuf,
    held: Vec<Held>,
    missing: Vec<PathBuf>,
}

pub struct Executables {
    held: Vec<Held>,
}

impl Executables {
    pub fn verify(&self) -> Result<()> {
        for input in &self.held {
            input.verify()?;
        }
        Ok(())
    }

    pub fn report(&self) -> Value {
        json!(self.held.iter().map(Held::report).collect::<Vec<_>>())
    }
}

pub fn validate_executables(inputs: &[(&str, &Path)]) -> Result<Executables> {
    require(
        inputs.len() == 4,
        "modem QA requires daemon, checker, trace, and process launcher ELFs",
    )?;
    let mut held = Vec::with_capacity(inputs.len());
    for (name, path) in inputs {
        let canonical = path.canonicalize()?;
        require(
            canonical.as_path() == *path,
            format!("modem executable path must be canonical: {name}"),
        )?;
        held.push(Held::executable(&canonical, name)?);
    }
    let receipt = Executables { held };
    receipt.verify()?;
    Ok(receipt)
}

impl Receipt {
    pub fn verify(&self) -> Result<()> {
        require(
            sha(MANIFEST) == MANIFEST_SHA256,
            "compiled modem capture manifest changed",
        )?;
        for input in &self.held {
            input.verify()?;
        }
        for path in &self.missing {
            require(
                fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                format!("retired modem source reappeared: {}", path.display()),
            )?;
        }
        Ok(())
    }

    pub fn expected(&self, name: &str) -> Result<Value> {
        let (_, _) = CASES
            .iter()
            .find(|(case, _)| *case == name)
            .ok_or_else(|| format!("unknown modem case: {name}"))?;
        let path = self.fixture.join("source").join(format!("{name}.json"));
        let held = self
            .held
            .iter()
            .find(|input| input.path == path)
            .ok_or("modem source expectation was not admitted")?;
        let mut file = held.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(held.len as usize);
        file.take(MAX_EXPECTATION + 1).read_to_end(&mut bytes)?;
        require(
            bytes.len() as u64 == held.len && sha(&bytes) == held.sha256,
            format!("modem expectation changed while reading: {name}"),
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn original_process_startup(&self) -> Result<Value> {
        let input = self
            .held
            .iter()
            .find(|input| input.name == "process-boundary-source.json")
            .ok_or("modem source process-boundary observation was not admitted")?;
        let mut file = input.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(input.len as usize);
        file.take(MAX_EXPECTATION + 1).read_to_end(&mut bytes)?;
        require(
            bytes.len() as u64 == input.len && sha(&bytes) == PROCESS_BOUNDARY_SHA256,
            "modem original process-boundary observation changed",
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn report(&self) -> Value {
        json!({"manifest_sha256": MANIFEST_SHA256,
          "artifact": 12389225409u64, "artifact_run": 39691148023u64,
          "source_commit": "d39b1b08c2079e9b3356be06ddddf9db49551be6",
          "source_bundle_sha256": "4285c91ffd0b45568d79472a782c8ca05a4cd475eca8cb7ce196af65d74f911b",
          "source_files": self.held.iter().filter(|input| input.path.starts_with(&self.root.join("openpilot")) || input.path.starts_with(&self.root.join("rust/tools"))).map(Held::report).collect::<Vec<_>>(),
          "original_oracles": self.held.iter().filter(|input| input.path.starts_with(&self.fixture) && input.name != "capture-manifest.json").map(Held::report).collect::<Vec<_>>(),
          "retired_sources_absent": self.missing})
    }
}

/// Admit the fixed source-only oracle and exact source files before native execution.
pub fn validate(root: &Path, require_retired: bool) -> Result<Receipt> {
    require(
        sha(MANIFEST) == MANIFEST_SHA256,
        "compiled modem capture manifest changed",
    )?;
    let root = root.canonicalize()?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    require(
        manifest["artifact"] == 12389225409u64
            && manifest["run"] == 39691148023u64
            && manifest["source_commit"] == "d39b1b08c2079e9b3356be06ddddf9db49551be6"
            && manifest["artifact_sha256"]
                == "4285c91ffd0b45568d79472a782c8ca05a4cd475eca8cb7ce196af65d74f911b",
        "modem capture provenance differs",
    )?;
    let source_pins = manifest["source_files"]
        .as_object()
        .ok_or("modem source pin roster is missing")?;
    require(
        source_pins.len() == SOURCE_PINS.len()
            && SOURCE_PINS
                .iter()
                .all(|(path, hash)| source_pins.get(*path).and_then(Value::as_str) == Some(hash)),
        "modem source pin roster differs",
    )?;
    let source_expectations = manifest["source_expectations"]
        .as_object()
        .ok_or("modem original-source expectations are missing")?;
    require(
        source_expectations.len() == CASES.len()
            && CASES.iter().all(|(name, hash)| {
                source_expectations
                    .get(format!("{name}.json"))
                    .and_then(Value::as_str)
                    == Some(*hash)
            }),
        "modem source expectation roster differs",
    )?;
    let fixture = root.join("rust/crates/dev-tools/tests/fixtures/modem-original");
    require(
        fixture.canonicalize()? == fixture,
        "modem original fixture must stay at its fixed repository path",
    )?;
    let mut held = Vec::new();
    let manifest_path = fixture.join("capture-manifest.json");
    let manifest_file = Held::open(
        &manifest_path,
        "capture-manifest.json".into(),
        MAX_EXPECTATION,
    )?;
    require(
        manifest_file.sha256 == MANIFEST_SHA256 && manifest_file.len == MANIFEST.len() as u64,
        "modem capture manifest differs from its compiled pin",
    )?;
    held.push(manifest_file);
    for (name, hash) in CASES {
        let relative = format!("source/{name}.json");
        let path = safe_path(&fixture, &relative)?;
        let input = Held::open(&path, relative.clone(), MAX_EXPECTATION)?;
        require(
            input.sha256 == hash,
            format!("modem original-source expectation changed: {name}"),
        )?;
        let mut bytes = Vec::with_capacity(input.len as usize);
        input
            .file
            .try_clone()?
            .take(MAX_EXPECTATION + 1)
            .read_to_end(&mut bytes)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        require(
            value["rows"].is_array() && value["commands"].is_array() && value["calls"].is_array(),
            format!("modem source expectation has incomplete trace fields: {name}"),
        )?;
        held.push(input);
    }
    let process_boundary_path = fixture.join("process-boundary-source.json");
    let process_boundary = Held::open(
        &process_boundary_path,
        "process-boundary-source.json".into(),
        MAX_EXPECTATION,
    )?;
    require(
        process_boundary.sha256 == PROCESS_BOUNDARY_SHA256,
        "modem original process-boundary observation differs",
    )?;
    let mut process_bytes = Vec::with_capacity(process_boundary.len as usize);
    process_boundary
        .file
        .try_clone()?
        .take(MAX_EXPECTATION + 1)
        .read_to_end(&mut process_bytes)?;
    let original_process: Value = serde_json::from_slice(&process_bytes)?;
    require(
        original_process["source_error"]
            .as_str()
            .is_some_and(|error| error.starts_with("[Errno 2]"))
            && original_process["external_commands"] == json!([]),
        "modem original startup-order observation differs",
    )?;
    held.push(process_boundary);
    let mut missing = Vec::new();
    for (name, hash) in SOURCE_PINS {
        let path = safe_path(&root, name)?;
        if require_retired {
            require(
                fs::symlink_metadata(&path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                format!("retired modem source remains: {name}"),
            )?;
            missing.push(path);
        } else {
            let source = Held::open(&path, name.to_owned(), MAX_SOURCE)?;
            require(
                source.sha256 == hash,
                format!("modem source pin changed: {name}"),
            )?;
            held.push(source);
        }
    }
    let receipt = Receipt {
        root,
        fixture,
        held,
        missing,
    };
    receipt.verify()?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).unwrap();
        for item in fs::read_dir(source).unwrap() {
            let item = item.unwrap();
            let target = destination.join(item.file_name());
            let metadata = item.file_type().unwrap();
            if metadata.is_dir() {
                copy_directory(&item.path(), &target);
            } else {
                assert!(
                    metadata.is_file(),
                    "unexpected fixture entry: {}",
                    item.path().display()
                );
                fs::copy(item.path(), target).unwrap();
            }
        }
    }

    fn checkout_copy() -> TempDir {
        let root = TempDir::new().unwrap();
        let repository = repository_root();
        for (name, _) in SOURCE_PINS {
            let source = repository.join(name);
            let target = root.path().join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(source, target).unwrap();
        }
        let fixture = repository.join("rust/crates/dev-tools/tests/fixtures/modem-original");
        copy_directory(
            &fixture,
            &root
                .path()
                .join("rust/crates/dev-tools/tests/fixtures/modem-original"),
        );
        root
    }

    #[test]
    fn original_capture_admission_rejects_mutated_expectation_after_hold() {
        let root = checkout_copy();
        let receipt = validate(root.path(), false).unwrap();
        receipt.verify().unwrap();
        let path = root
            .path()
            .join("rust/crates/dev-tools/tests/fixtures/modem-original/source/normal.json");
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(path, bytes).unwrap();
        assert!(receipt.verify().is_err());
    }

    #[test]
    fn retired_admission_rejects_reappearing_source_and_fixture_symlink() {
        let root = checkout_copy();
        for (name, _) in SOURCE_PINS {
            fs::remove_file(root.path().join(name)).unwrap();
        }
        let receipt = validate(root.path(), true).unwrap();
        let returned = root.path().join(SOURCE_PINS[0].0);
        fs::write(&returned, b"returned retired Python source").unwrap();
        assert!(receipt.verify().is_err());

        let second = checkout_copy();
        let fixture = second
            .path()
            .join("rust/crates/dev-tools/tests/fixtures/modem-original/source");
        fs::remove_file(fixture.join("normal.json")).unwrap();
        symlink("at_errors.json", fixture.join("normal.json")).unwrap();
        assert!(validate(second.path(), false).is_err());
    }

    #[test]
    fn executable_admission_requires_held_current_host_elf_identity() {
        let current = std::env::current_exe().unwrap();
        let inputs = [
            ("daemon", current.as_path()),
            ("checker", current.as_path()),
            ("trace", current.as_path()),
            ("launcher", current.as_path()),
        ];
        let held = validate_executables(&inputs).unwrap();
        held.verify().unwrap();

        let directory = TempDir::new().unwrap();
        let text = directory.path().join("not-elf");
        fs::write(&text, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&text).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&text, permissions).unwrap();
        let invalid = [
            ("daemon", text.as_path()),
            ("checker", current.as_path()),
            ("trace", current.as_path()),
            ("launcher", current.as_path()),
        ];
        assert!(validate_executables(&invalid).is_err());

        let link = directory.path().join("elf-link");
        symlink(&current, &link).unwrap();
        let linked = [
            ("daemon", link.as_path()),
            ("checker", current.as_path()),
            ("trace", current.as_path()),
            ("launcher", current.as_path()),
        ];
        assert!(validate_executables(&linked).is_err());
    }
}
