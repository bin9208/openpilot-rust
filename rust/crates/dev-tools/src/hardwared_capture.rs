//! Admission and final revalidation for the independent original hardwared QA capture.
use crate::seal::Result;
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::ffi::OsStrExt,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const MANIFEST: &[u8] =
    include_bytes!("../tests/fixtures/hardwared-original/capture-manifest.json");
const MANIFEST_SHA256: &str = "f68ba0febd2e146e4ae0bfb1f3e54e1b545da9a7e8a894d257a9d1388022bf9d";
const SOURCE_SHA256: &str = "92ed8579d96b4e555c8c9d8a1452de120da1944c60f189864d04e2cbc3a7ce89";
const SOURCE_GZIP_SHA256: &str = "648961bfb0ae0c20d3a8d8e07b999e4e6dd65ae121f6f9bba38a963fae35c6d6";
const SOURCE_BYTES: u64 = 264_722;
const SOURCE_GZIP_BYTES: u64 = 8_797;
const INPUTS: usize = 891;
const OBSERVATIONS: usize = 890;
const MAX_PINNED_SOURCE: u64 = 32 * 1024 * 1024;
const MAX_EXECUTABLE: u64 = 512 * 1024 * 1024;
const MAX_CAPTURE: u64 = 1 * 1024 * 1024;
const RETIRED_TOOLS: [&str; 3] = [
    "rust/tools/check_hardwared.py",
    "rust/tools/hardwared_source.py",
    "rust/tools/check_hardwared_daemon.py",
];

const CASES: [(&str, usize, usize); 12] = [
    ("fan-tici", 216, 216),
    ("fan-tizi", 216, 216),
    ("fan-mici", 216, 216),
    ("normal-cycle-disconnect", 12, 12),
    ("panda-disconnect-timeout", 5, 5),
    ("startup-block-and-boot-latch", 9, 9),
    ("thermal-hysteresis", 154, 154),
    ("offroad-danger-boundary", 5, 5),
    ("tesla-awake-after-start", 9, 9),
    ("ignition-edge-between-ticks", 6, 5),
    ("power-integration-shutdown-boundaries", 13, 13),
    ("low-voltage-shutdown", 30, 30),
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

fn safe_relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    require(
        !name.is_empty()
            && !name.contains('\\')
            && !name.contains('\0')
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        format!("unsafe hardwared capture path: {name}"),
    )?;
    Ok(path)
}

struct HeldFile {
    label: String,
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    len: u64,
    mode: u32,
    mtime: (i64, i64),
    ctime: (i64, i64),
    sha256: String,
    max: u64,
}

fn metadata_identity(m: &std::fs::Metadata) -> (u64, u64, u64, u32, (i64, i64), (i64, i64)) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        (m.mtime(), m.mtime_nsec()),
        (m.ctime(), m.ctime_nsec()),
    )
}

impl HeldFile {
    fn open(path: &Path, label: String, max: u64) -> Result<Self> {
        for parent in path.ancestors().skip(1) {
            let metadata = fs::symlink_metadata(parent)?;
            require(
                metadata.file_type().is_dir(),
                format!(
                    "hardwared input parent is not a real directory: {}",
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
            metadata.is_file() && metadata.len() <= max,
            format!("hardwared input is nonregular or oversized: {label}"),
        )?;
        let mut held = Self {
            label,
            path: path.to_path_buf(),
            file,
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            mode: metadata.mode(),
            mtime: (metadata.mtime(), metadata.mtime_nsec()),
            ctime: (metadata.ctime(), metadata.ctime_nsec()),
            sha256: String::new(),
            max,
        };
        let before = metadata_identity(&metadata);
        held.sha256 = held.digest()?;
        let after = held.file.metadata()?;
        require(
            metadata_identity(&after) == before,
            format!("hardwared input changed while hashing: {}", held.label),
        )?;
        Ok(held)
    }

    fn digest(&self) -> Result<String> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut bytes = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or("hardwared input length overflow")?;
            require(total <= self.max, "hardwared input exceeds read bound")?;
            hash.update(&bytes[..count]);
        }
        require(total == self.len, "hardwared held input length changed")?;
        Ok(format!("{:x}", hash.finalize()))
    }

    fn verify(&self) -> Result<()> {
        let metadata = self.file.metadata()?;
        require(
            metadata.is_file() && metadata_identity(&metadata) == self.identity(),
            format!("held hardwared input identity changed: {}", self.label),
        )?;
        require(
            self.digest()? == self.sha256,
            format!("held hardwared input content changed: {}", self.label),
        )?;
        require(
            metadata_identity(&self.file.metadata()?) == self.identity(),
            format!(
                "held hardwared input metadata changed while rehashing: {}",
                self.label
            ),
        )?;
        let current = HeldFile::open(&self.path, self.label.clone(), self.max)?;
        require(
            current.identity() == self.identity() && current.sha256 == self.sha256,
            format!("hardwared input path was replaced: {}", self.label),
        )
    }

    fn executable(path: &Path, label: String) -> Result<Self> {
        let held = Self::open(path, label, MAX_EXECUTABLE)?;
        require(
            held.mode & 0o111 != 0,
            format!(
                "hardwared executable lacks execute permission: {}",
                held.label
            ),
        )?;
        let mut file = held.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut elf = [0u8; 20];
        file.read_exact(&mut elf)?;
        require(
            &elf[..4] == b"\x7fELF"
                && elf[4] == 2
                && elf[5] == 1
                && elf[6] == 1
                && matches!(u16::from_le_bytes([elf[16], elf[17]]), 2 | 3)
                && matches!(u16::from_le_bytes([elf[18], elf[19]]), 62 | 183),
            format!(
                "hardwared executable is not a supported 64-bit little-endian ELF: {}",
                held.label
            ),
        )?;
        held.verify()?;
        Ok(held)
    }

    fn machine(&self) -> Result<u16> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(18))?;
        let mut bytes = [0u8; 2];
        file.read_exact(&mut bytes)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn identity(&self) -> (u64, u64, u64, u32, (i64, i64), (i64, i64)) {
        (
            self.dev, self.ino, self.len, self.mode, self.mtime, self.ctime,
        )
    }

    fn report(&self) -> Value {
        json!({"path":self.label,"bytes":self.len,"sha256":self.sha256,"device":self.dev,"inode":self.ino,"mode":self.mode,"mtime":self.mtime,"ctime":self.ctime})
    }
}

pub struct Receipt {
    evidence_manifest: HeldFile,
    original: HeldFile,
    binary: HeldFile,
    executables: Vec<HeldFile>,
    runner_argv: Vec<Vec<u8>>,
    sources: Vec<HeldFile>,
    missing: Vec<PathBuf>,
    require_retired: bool,
    input_rows: usize,
    observation_rows: usize,
}

impl Receipt {
    pub fn original_json_bytes(&self) -> Result<Vec<u8>> {
        let mut compressed = self.original.file.try_clone()?;
        compressed.seek(SeekFrom::Start(0))?;
        let mut decoded = Vec::new();
        GzDecoder::new(compressed)
            .take(SOURCE_BYTES + 1)
            .read_to_end(&mut decoded)?;
        require(
            decoded.len() as u64 == SOURCE_BYTES && sha(&decoded) == SOURCE_SHA256,
            "held hardwared original source capture differs",
        )?;
        Ok(decoded)
    }

    pub fn verify(&self) -> Result<()> {
        require(
            sha(MANIFEST) == MANIFEST_SHA256,
            "compiled hardwared capture manifest changed",
        )?;
        self.evidence_manifest.verify()?;
        self.original.verify()?;
        self.binary.verify()?;
        for executable in &self.executables {
            executable.verify()?;
        }
        for source in &self.sources {
            source.verify()?;
        }
        for path in &self.missing {
            require_absent(path)?;
        }
        Ok(())
    }

    pub fn report(&self) -> Value {
        json!({
            "manifest_sha256":MANIFEST_SHA256,
            "original_source_sha256":SOURCE_SHA256,
            "original_source_gzip_sha256":SOURCE_GZIP_SHA256,
            "binary":self.binary.report(),
            "runner_and_launcher":self.executables.iter().map(HeldFile::report).collect::<Vec<_>>(),
            "runner_argv_bytes":self.runner_argv,
            "require_retired_tools":self.require_retired,
            "absent_retired_tools":self.missing.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
            "source_pins":self.sources.iter().map(HeldFile::report).collect::<Vec<_>>(),
            "input_steps":self.input_rows,
            "expected_observations":self.observation_rows,
        })
    }
}

/// Admit the fixed independent capture and current source closure before a run.
pub fn validate(
    root: &Path,
    evidence: &Path,
    binary: &Path,
    runner: &[OsString],
    launcher: Option<&Path>,
    require_retired: bool,
) -> Result<Receipt> {
    require(
        sha(MANIFEST) == MANIFEST_SHA256,
        "compiled hardwared capture manifest changed",
    )?;
    let root = root.canonicalize()?;
    let evidence = evidence.canonicalize()?;
    require(
        evidence.starts_with(&root),
        "hardwared evidence is outside source checkout",
    )?;
    require(
        evidence == root.join("rust/crates/dev-tools/tests/fixtures/hardwared-original"),
        "hardwared evidence must use the fixed in-repository capture",
    )?;
    let binary = binary.canonicalize()?;
    let manifest_path = evidence.join("capture-manifest.json");
    let evidence_manifest =
        HeldFile::open(&manifest_path, "capture-manifest.json".into(), MAX_CAPTURE)?;
    require(
        evidence_manifest.sha256 == MANIFEST_SHA256
            && evidence_manifest.file.metadata()?.len() == MANIFEST.len() as u64,
        "hardwared capture manifest differs from compiled pin",
    )?;
    let manifest: Value = serde_json::from_slice(MANIFEST)?;
    require(
        manifest["schema_version"] == 1
            && manifest["input_steps_total"] == INPUTS
            && manifest["expected_observations_total"] == OBSERVATIONS
            && manifest["source_pins"]
                .as_object()
                .is_some_and(|pins| pins.len() == 19),
        "hardwared source capture manifest contract differs",
    )?;
    let original = HeldFile::open(
        &evidence.join("source.json.gz"),
        "source.json.gz".into(),
        MAX_CAPTURE,
    )?;
    require(
        original.len == SOURCE_GZIP_BYTES && original.sha256 == SOURCE_GZIP_SHA256,
        "hardwared original compressed capture differs",
    )?;
    let mut decoded = Vec::new();
    let mut compressed = original.file.try_clone()?;
    compressed.seek(SeekFrom::Start(0))?;
    GzDecoder::new(compressed)
        .take(SOURCE_BYTES + 1)
        .read_to_end(&mut decoded)?;
    require(
        decoded.len() as u64 == SOURCE_BYTES && sha(&decoded) == SOURCE_SHA256,
        "hardwared original source capture bytes differ",
    )?;
    let rows: Vec<Value> = serde_json::from_slice(&decoded)?;
    require(
        rows.len() == CASES.len(),
        "hardwared source capture case count differs",
    )?;
    let mut inputs = 0usize;
    let mut observations = 0usize;
    for (row, (name, expected_inputs, expected_outputs)) in rows.iter().zip(CASES) {
        let input_count = row["input"]["steps"]
            .as_array()
            .ok_or("hardwared captured input lacks steps")?
            .len();
        let output_count = row["expected"]
            .as_array()
            .ok_or("hardwared captured output is not an array")?
            .len();
        require(
            row["name"] == name
                && input_count == expected_inputs
                && output_count == expected_outputs,
            format!("hardwared captured case differs: {name}"),
        )?;
        inputs += input_count;
        observations += output_count;
    }
    require(
        inputs == INPUTS && observations == OBSERVATIONS,
        "hardwared capture row totals differ",
    )?;
    let binary_pin = HeldFile::executable(&binary, "current-native-binary".into())?;
    let mut executables = Vec::new();
    if let Some(program) = runner.first() {
        let path = Path::new(program);
        require(
            path.is_absolute(),
            "hardwared runner executable must be absolute",
        )?;
        executables.push(HeldFile::executable(path, "runner[0]".into())?);
    }
    if let Some(path) = launcher {
        require(path.is_absolute(), "hardwared launcher must be absolute")?;
        executables.push(HeldFile::executable(path, "daemon-launcher".into())?);
    }
    let host_machine = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("unsupported hardwared QA host architecture".into()),
    };
    if binary_pin.machine()? != host_machine {
        require(
            !runner.is_empty(),
            "foreign hardwared binary requires an explicit host runner",
        )?;
    }
    if !runner.is_empty() {
        require(
            executables[0].machine()? == host_machine,
            "hardwared runner is not a current-host ELF",
        )?;
    }
    if launcher.is_some() {
        require(
            executables
                .last()
                .ok_or("missing held hardwared launcher")?
                .machine()?
                == host_machine,
            "hardwared launcher is not a current-host ELF",
        )?;
    }
    let mut sources = Vec::new();
    let mut missing = Vec::new();
    let pins = manifest["source_pins"]
        .as_object()
        .ok_or("hardwared source pins missing")?;
    let expected_paths: [&str; 19] = [
        "rust/tools/check_hardwared.py",
        "rust/tools/hardwared_source.py",
        "rust/tools/check_hardwared_daemon.py",
        "openpilot/common/pid.py",
        "openpilot/common/filter_simple.py",
        "openpilot/system/hardware/fan_controller.py",
        "openpilot/system/hardware/power_monitoring.py",
        "openpilot/system/hardware/hardwared.py",
        "openpilot/cereal/__init__.py",
        "openpilot/cereal/services.py",
        "openpilot/cereal/log.capnp",
        "openpilot/cereal/custom.capnp",
        "openpilot/cereal/deprecated.capnp",
        "openpilot/cereal/include/c++.capnp",
        "opendbc_repo/opendbc/car/car.capnp",
        "opendbc_repo/opendbc/car/include/c++.capnp",
        "pyproject.toml",
        "uv.lock",
        "msgq_repo/msgq/__init__.py",
    ];
    require(
        pins.len() == expected_paths.len()
            && expected_paths.iter().all(|name| pins.contains_key(*name)),
        "hardwared source pin roster differs",
    )?;
    for name in expected_paths {
        let path = root.join(safe_relative(name)?);
        let retired_path = RETIRED_TOOLS.contains(&name);
        let absent_now = fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
        if retired_path && (require_retired || absent_now) {
            require_absent(&path)?;
            missing.push(path);
            continue;
        }
        let source = HeldFile::open(&path, name.to_owned(), MAX_PINNED_SOURCE)?;
        let expected = pins[name]["sha256"]
            .as_str()
            .ok_or("hardwared source pin missing SHA")?;
        let expected_len = pins[name]["bytes"]
            .as_u64()
            .ok_or("hardwared source pin missing length")?;
        require(
            source.sha256 == expected && source.len == expected_len,
            format!("hardwared source input changed: {name}"),
        )?;
        sources.push(source);
    }
    let receipt = Receipt {
        evidence_manifest,
        original,
        binary: binary_pin,
        executables,
        runner_argv: runner
            .iter()
            .map(|argument| argument.as_bytes().to_vec())
            .collect(),
        sources,
        missing,
        require_retired,
        input_rows: inputs,
        observation_rows: observations,
    };
    // The compiled bytes are independently checked on every verification pass.
    require(
        sha(MANIFEST) == MANIFEST_SHA256,
        "compiled hardwared manifest changed",
    )?;
    receipt.verify()?;
    Ok(receipt)
}

fn require_absent(path: &Path) -> Result<()> {
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent)?;
        require(
            metadata.file_type().is_dir(),
            format!(
                "retired hardwared source parent is not a real directory: {}",
                parent.display()
            ),
        )?;
    }
    require(
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        format!("retired hardwared source remains: {}", path.display()),
    )
}
