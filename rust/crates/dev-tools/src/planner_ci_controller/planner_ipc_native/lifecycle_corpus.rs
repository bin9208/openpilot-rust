//! lifecycle schema1의 독립 관측 reader. source는 승인된 실제841 audit/index의 일치로 확인한다.
use super::corpus::{Artifact, RawFile};
use crate::seal::Result;
use rustix::fs::{open, Mode, OFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

pub(super) const MIB: u64 = 1024 * 1024;
// Hypatia가 원래841 archive에서 작성한 audit JSON 원문 bytes의 SHA다. source는 이 audit에 결합한다.
const QUALIFIED_AUDITS: &[(&str, &str)] = &[
    (
        "x86_64",
        "9ae47494dcf2e201fa1c48a70e6acd934e08fefc008001bd0c95c9ba502eb88e",
    ),
    (
        "aarch64",
        "23bace6376cc1d87a1857bc8f69b64d6f0ddf5c526e1f69a133502620bd81f80",
    ),
];
pub(super) const SEND_ORDER: [&str; 9] = [
    "carControl",
    "carState",
    "controlsState",
    "liveParameters",
    "radarState",
    "selfdriveState",
    "carrotMan",
    "livePose",
    "modelV2",
];
const SOURCES: [&str; 14] = [
    "rust/tools/check_plannerd_lifecycle.py",
    "rust/tools/plannerd_lifecycle_peer.py",
    "rust/tools/plannerd_ipc.py",
    "rust/tools/plannerd_owner_fixtures.py",
    "rust/tools/plannerd_daemon_source.py",
    "rust/tools/plannerd_owner_loader.py",
    "openpilot/selfdrive/controls/plannerd.py",
    "openpilot/common/params_keys.h",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "openpilot/cereal/services.py",
    "openpilot/selfdrive/modeld/constants.py",
    "openpilot/cereal/messaging/__init__.py",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Case {
    pub name: &'static str,
    pub phase: &'static str,
    pub signal: i32,
    pub car_params: &'static str,
}
pub(super) const CASES: [Case; 13] = [
    Case {
        name: "wait-missing-sigint",
        phase: "wait",
        signal: 2,
        car_params: "missing",
    },
    Case {
        name: "wait-missing-sigterm",
        phase: "wait",
        signal: 15,
        car_params: "missing",
    },
    Case {
        name: "wait-directory-sigint",
        phase: "wait",
        signal: 2,
        car_params: "directory",
    },
    Case {
        name: "wait-directory-sigterm",
        phase: "wait",
        signal: 15,
        car_params: "directory",
    },
    Case {
        name: "wait-empty-sigint",
        phase: "wait",
        signal: 2,
        car_params: "empty",
    },
    Case {
        name: "wait-empty-sigterm",
        phase: "wait",
        signal: 15,
        car_params: "empty",
    },
    Case {
        name: "ready-sigint",
        phase: "ready",
        signal: 2,
        car_params: "missing",
    },
    Case {
        name: "ready-sigterm",
        phase: "ready",
        signal: 15,
        car_params: "missing",
    },
    Case {
        name: "recover-directory",
        phase: "recover",
        signal: 15,
        car_params: "directory",
    },
    Case {
        name: "settings-directories",
        phase: "ready",
        signal: 15,
        car_params: "missing",
    },
    Case {
        name: "fatal-integer",
        phase: "integer",
        signal: 15,
        car_params: "missing",
    },
    Case {
        name: "fatal-float",
        phase: "float",
        signal: 15,
        car_params: "missing",
    },
    Case {
        name: "malformed-CarParams",
        phase: "malformed",
        signal: 15,
        car_params: "missing",
    },
];
pub(super) const SETTINGS: [&str; 4] = [
    "EnableRadarTracks",
    "UseLaneLineSpeed",
    "LatMpcPathCost",
    "LongActuatorDelay",
];
impl Case {
    pub fn frames(self) -> usize {
        match self.phase {
            "ready" | "recover" => 3,
            "float" => 1,
            _ => 0,
        }
    }
    pub fn expected(self, mode: &str) -> i32 {
        match self.phase {
            "integer" | "float" => {
                if mode == "source" {
                    -6
                } else {
                    1
                }
            }
            "malformed" => 1,
            _ if self.phase == "wait" || self.signal == 2 => {
                if mode == "source" {
                    -2
                } else {
                    0
                }
            }
            _ => -15,
        }
    }
}

#[derive(Clone, Deserialize, Serialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CaseResult {
    pub case: String,
    pub mode: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publications: Option<Vec<String>>,
    pub returncode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_returncode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexedRun {
    case: String,
    mode: String,
    epoch: usize,
    run_index: usize,
    manifest: RawFile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Admission {
    free_bytes: u64,
    existing_bytes: u64,
    capture_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    source_commit: String,
    source_sha256: BTreeMap<String, String>,
    owner: String,
    environment: BTreeMap<String, String>,
    identities: BTreeMap<String, Identity>,
    sdk_before_proof: BTreeMap<String, Identity>,
    source_native: String,
    python_argv: String,
    python_executable: String,
    python_version: String,
    admission: Admission,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Usage {
    payload_bytes: u64,
    apparent_bytes: u64,
    du_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalReceipt {
    status: String,
    cases: usize,
    binary_sha256: String,
    scope: String,
    exit_boundary: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: u32,
    kind: String,
    status: String,
    provenance: Provenance,
    runs: Vec<IndexedRun>,
    inputs: usize,
    params_snapshots: usize,
    receipt_sha256: String,
    usage_before_index: Usage,
    capture_limit_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Packet {
    pub frame: usize,
    pub service: String,
    pub raw: RawFile,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum State {
    #[serde(rename = "absent")]
    Absent {},
    #[serde(rename = "dir")]
    Directory {},
    #[serde(rename = "file")]
    File { raw: RawFile },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    monotonic: f64,
    pub entries: BTreeMap<String, State>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Run {
    pub case: String,
    pub mode: String,
    epoch: usize,
    run_index: usize,
    phase: String,
    signal: i32,
    car_params: String,
    directories: Vec<String>,
    path: String,
    prefix: String,
    argv: Vec<String>,
    environment: BTreeMap<String, String>,
    frames: usize,
    pub params: BTreeMap<String, Snapshot>,
    pub inputs: Vec<Packet>,
    param_names: Vec<String>,
    result: CaseResult,
    result_receipt_sha256: String,
}
/// 업로드 뒤 감사자가 작성한다. producer 내부에 존재하지 않는 artifact ID를 추정하지 않는다.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audit {
    schema_version: u32,
    kind: String,
    source_commit: String,
    host_architecture: String,
    origin_run_id: String,
    origin_run_attempt: String,
    origin_job: String,
    source_sha256: BTreeMap<String, String>,
    artifact: Artifact,
    index: RawFile,
    receipt: RawFile,
    cases: RawFile,
    runs: Vec<AuditRun>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRun {
    receipt: RawFile,
    publications: Vec<Publication>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    frame: usize,
    service: String,
    raw: RawFile,
}

pub(super) fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', '\0'])
        && value
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && Path::new(value)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}
fn identity(a: &Metadata, b: &Metadata) -> bool {
    (
        a.dev(),
        a.ino(),
        a.mode(),
        a.uid(),
        a.gid(),
        a.nlink(),
        a.len(),
        a.mtime(),
        a.mtime_nsec(),
        a.ctime(),
        a.ctime_nsec(),
    ) == (
        b.dev(),
        b.ino(),
        b.mode(),
        b.uid(),
        b.gid(),
        b.nlink(),
        b.len(),
        b.mtime(),
        b.mtime_nsec(),
        b.ctime(),
        b.ctime_nsec(),
    )
}
pub(super) struct HeldFile {
    path: PathBuf,
    raw: RawFile,
    before: Metadata,
    limit: u64,
}
impl HeldFile {
    pub fn open(root: &Path, raw: &RawFile, limit: u64, retain: bool) -> Result<(Vec<u8>, Self)> {
        if !relative(&raw.path)
            || !sha(&raw.sha256)
            || raw.bytes > limit
            || root.canonicalize()? != root
        {
            return Err("invalid lifecycle file descriptor/path/bound".into());
        }
        let path = root.join(&raw.path);
        if path.canonicalize()? != path {
            return Err("lifecycle corpus symlink rejected".into());
        }
        let mut file = File::from(open(
            &path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let before = file.metadata()?;
        if !before.is_file() || before.len() != raw.bytes {
            return Err("lifecycle regular file size differs".into());
        }
        let mut bytes = Vec::new();
        let mut digest = Sha256::new();
        let mut count = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > raw.bytes {
                return Err("lifecycle file grew while reading".into());
            }
            digest.update(&buffer[..n]);
            if retain {
                bytes.extend_from_slice(&buffer[..n]);
            }
        }
        if count != raw.bytes
            || format!("{:x}", digest.finalize()) != raw.sha256
            || !identity(&before, &file.metadata()?)
            || !identity(&before, &fs::symlink_metadata(&path)?)
        {
            return Err("lifecycle file SHA/FD/path identity differs".into());
        }
        Ok((
            bytes,
            Self {
                path,
                raw: raw.clone(),
                before,
                limit,
            },
        ))
    }
    pub fn verify(&self) -> Result<()> {
        let (_, current) = Self::open(
            self.path.parent().ok_or("missing file parent")?,
            &RawFile {
                path: self
                    .path
                    .file_name()
                    .ok_or("missing file name")?
                    .to_str()
                    .ok_or("file name UTF-8")?
                    .into(),
                bytes: self.raw.bytes,
                sha256: self.raw.sha256.clone(),
            },
            self.limit,
            false,
        )?;
        if !identity(&self.before, &current.before) {
            return Err("lifecycle file reappeared".into());
        }
        Ok(())
    }
}

pub(super) fn qualified_audit(host: &str) -> Result<&'static str> {
    QUALIFIED_AUDITS
        .iter()
        .find_map(|(name, pin)| (*name == host).then_some(*pin))
        .ok_or_else(|| {
            "lifecycle original corpus has no qualified external audit for this host".into()
        })
}

pub(super) struct Capture {
    root: PathBuf,
    capture_root: PathBuf,
    held: Vec<HeldFile>,
    members: BTreeMap<String, RawFile>,
    directories: Vec<(PathBuf, Metadata)>,
    pub runs: Vec<Run>,
    pub audit_sha256: String,
}
impl Capture {
    pub fn open(bundle: &Path, audit_path: &Path) -> Result<Self> {
        let pin = qualified_audit(std::env::consts::ARCH)?;
        let raw = RawFile {
            path: audit_path
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("audit filename")?
                .into(),
            bytes: fs::symlink_metadata(audit_path)?.len(),
            sha256: pin.into(),
        };
        let (bytes, audit_file) =
            HeldFile::open(audit_path.parent().ok_or("audit parent")?, &raw, MIB, true)?;
        Self::admit(bundle, &bytes, audit_file, pin)
    }
    fn admit(bundle: &Path, bytes: &[u8], audit_file: HeldFile, pin: &str) -> Result<Self> {
        let audit: Audit = serde_json::from_slice(bytes)?;
        if audit.schema_version != 1
            || audit.kind != "audited-original-planner-lifecycle-inputs"
            || audit.source_commit.len() != 40
            || !audit
                .source_commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || audit.host_architecture != std::env::consts::ARCH
            || audit.origin_job != "planner-runtime"
            || audit.runs.len() != 26
            || audit.artifact.id == 0
            || audit.artifact.bytes == 0
            || !sha(&audit.artifact.sha256)
            || ![&audit.origin_run_id, &audit.origin_run_attempt]
                .iter()
                .all(|v| v.parse::<u64>().is_ok_and(|n| n > 0))
        {
            return Err("incomplete/unqualified lifecycle external audit".into());
        }
        let (_, archive) = HeldFile::open(
            bundle,
            &RawFile {
                path: audit.artifact.archive.clone(),
                bytes: audit.artifact.bytes,
                sha256: audit.artifact.sha256.clone(),
            },
            6 * 1024 * MIB,
            false,
        )?;
        if !relative(&audit.artifact.extracted) {
            return Err("invalid lifecycle extraction root".into());
        }
        let root = bundle.join(&audit.artifact.extracted);
        let (index_bytes, index_file) = HeldFile::open(&root, &audit.index, MIB, true)?;
        let index: Index = serde_json::from_slice(&index_bytes)?;
        validate_index(&index, &audit)?;
        let capture_root = root
            .join(&audit.index.path)
            .parent()
            .ok_or("capture root")?
            .to_owned();
        let comparison = Path::new(&audit.index.path)
            .parent()
            .and_then(Path::parent)
            .ok_or("lifecycle comparison directory")?;
        if Path::new(&audit.receipt.path) != comparison.join("lifecycle/receipt.json")
            || Path::new(&audit.cases.path) != comparison.join("lifecycle/cases.json")
        {
            return Err("lifecycle capture and original results are not siblings".into());
        }
        if capture_root.file_name().and_then(|v| v.to_str()) != Some("lifecycle-original-inputs") {
            return Err("wrong lifecycle capture root".into());
        }
        let (receipt_bytes, receipt_file) = HeldFile::open(&root, &audit.receipt, MIB, true)?;
        let receipt: OriginalReceipt = serde_json::from_slice(&receipt_bytes)?;
        if audit.receipt.sha256 != index.receipt_sha256
            || receipt.status != "PASS" || receipt.cases != 13
            || receipt.binary_sha256 != index.provenance.identities["binary"].sha256
            || receipt.scope != "real source/native lifecycle; native clean SIGINT matches the original manager launcher"
            || receipt.exit_boundary != "raw source KeyboardInterrupt=-2; invalid numeric source abort vs typed Rust exit1 remains explicit"
        {
            return Err("lifecycle original receipt/source binding differs".into());
        }
        let (cases_bytes, cases_file) = HeldFile::open(&root, &audit.cases, MIB, true)?;
        let results: Vec<CaseResult> = serde_json::from_slice(&cases_bytes)?;
        if results.len() != 26 {
            return Err("lifecycle requires original26 results".into());
        }
        let mut capture = Self {
            root,
            capture_root,
            held: vec![audit_file, archive, index_file, receipt_file, cases_file],
            members: BTreeMap::new(),
            directories: Vec::new(),
            runs: Vec::new(),
            audit_sha256: pin.into(),
        };
        let mut total_inputs = 0;
        let mut total_publications = 0;
        let mut total_params = 0;
        for (i, indexed) in index.runs.iter().enumerate() {
            let bytes = capture.read_capture(&indexed.manifest, MIB)?;
            let run: Run = serde_json::from_slice(&bytes)?;
            validate_run(&run, indexed, i)?;
            if run.result != results[i] {
                return Err("lifecycle original cases/manifest differ".into());
            }
            let original = &audit.runs[i];
            let expected_receipt = Path::new(&audit.receipt.path)
                .parent()
                .ok_or("receipt directory")?
                .join(&run.case)
                .join(&run.mode)
                .join("receipt.json");
            if Path::new(&original.receipt.path) != expected_receipt
                || original.receipt.sha256 != run.result_receipt_sha256
            {
                return Err("lifecycle case receipt escaped cohort".into());
            }
            bind_original_invocation(&run, &index.provenance, &expected_receipt)?;
            let (bytes, file) = HeldFile::open(&capture.root, &original.receipt, MIB, true)?;
            if serde_json::from_slice::<CaseResult>(&bytes)? != run.result {
                return Err("original case receipt differs".into());
            }
            capture.held.push(file);
            for snapshot in run.params.values() {
                let mut size = 0_u64;
                for (key, state) in &snapshot.entries {
                    if let State::File { raw } = state {
                        size = size
                            .checked_add(raw.bytes)
                            .ok_or("lifecycle snapshot size overflow")?;
                        let bytes = capture
                            .read_capture(raw, if key == "CarParams" { MIB } else { 64 * 1024 })?;
                        if ((run.case == "fatal-integer" && key == "EnableRadarTracks")
                            || (run.case == "fatal-float" && key == "LatMpcPathCost"))
                            && bytes != b"invalid"
                        {
                            return Err("original numeric fault literal differs".into());
                        }
                        if run.case == "malformed-CarParams" && key == "CarParams" && bytes != b"x"
                        {
                            return Err("original malformed CP literal differs".into());
                        }
                    }
                }
                if size > 4 * MIB {
                    return Err("lifecycle Params snapshot exceeds4MiB".into());
                }
            }
            for input in &run.inputs {
                capture.read_capture(&input.raw, MIB)?;
            }
            let mut frames = BTreeMap::<usize, BTreeSet<String>>::new();
            for (n, publication) in original.publications.iter().enumerate() {
                let (bytes, file) = HeldFile::open(&capture.root, &publication.raw, MIB, true)?;
                if !super::OUTPUTS.contains(&publication.service.as_str())
                    || publication.frame >= 3
                    || Path::new(&publication.raw.path)
                        != expected_receipt
                            .parent()
                            .ok_or("case directory")?
                            .join(format!(
                                "publication-{}-{n}-{}.bin",
                                publication.frame, publication.service
                            ))
                {
                    return Err("original lifecycle output escaped service/frame/case".into());
                }
                super::decode::packet(&bytes, &publication.service, publication.raw.path.clone())?;
                frames
                    .entry(publication.frame)
                    .or_default()
                    .insert(publication.service.clone());
                capture.held.push(file);
            }
            let services: Vec<_> = original
                .publications
                .iter()
                .map(|v| v.service.clone())
                .collect();
            if run
                .result
                .publications
                .as_ref()
                .is_some_and(|v| v != &services)
                || (!services.is_empty() && run.result.publications.is_none())
            {
                return Err("lifecycle raw publication order differs from original result".into());
            }
            let normal = matches!(CASES[i / 2].phase, "ready" | "recover");
            if (normal && (frames.len() != 3 || frames.values().any(|v| v.len() != 3)))
                || (!normal && !frames.is_empty())
            {
                return Err("original lifecycle service frame sets differ".into());
            }
            total_inputs += run.inputs.len();
            total_params += run.params.len();
            total_publications += original.publications.len();
            capture.runs.push(run);
        }
        if total_inputs != 234
            || total_params != 30
            || total_publications != 72
            || capture.members.values().map(|v| v.bytes).sum::<u64>() > 64 * MIB
        {
            return Err("incomplete lifecycle234/30/72 corpus".into());
        }
        capture.directories =
            seal_capture_tree(&capture.capture_root, &capture.members, &capture.runs)?;
        capture.verify()?;
        Ok(capture)
    }
    fn read_capture(&mut self, raw: &RawFile, limit: u64) -> Result<Vec<u8>> {
        if self.members.contains_key(&raw.path) || self.members.len() >= 62000 {
            return Err("duplicate lifecycle member".into());
        }
        let (bytes, held) = HeldFile::open(&self.capture_root, raw, limit, true)?;
        if held.before.nlink() != 1 {
            return Err("lifecycle capture hardlink alias rejected".into());
        }
        self.members.insert(raw.path.clone(), raw.clone());
        self.held.push(held);
        Ok(bytes)
    }
    pub fn bytes(&self, raw: &RawFile) -> Result<Vec<u8>> {
        let expected = self
            .members
            .get(&raw.path)
            .ok_or("unadmitted lifecycle member")?;
        if expected.bytes != raw.bytes || expected.sha256 != raw.sha256 {
            return Err("lifecycle raw descriptor changed".into());
        }
        self.held
            .iter()
            .find(|file| file.path == self.capture_root.join(&raw.path))
            .ok_or("missing lifecycle member identity")?
            .verify()?;
        Ok(HeldFile::open(&self.capture_root, raw, MIB, true)?.0)
    }
    pub fn verify(&self) -> Result<()> {
        for (path, before) in &self.directories {
            if !identity(before, &fs::symlink_metadata(path)?) || path.canonicalize()? != *path {
                return Err("lifecycle capture directory changed/reappeared".into());
            }
        }
        for file in &self.held {
            file.verify()?;
        }
        Ok(())
    }
}

fn seal_capture_tree(
    root: &Path,
    members: &BTreeMap<String, RawFile>,
    runs: &[Run],
) -> Result<Vec<(PathBuf, Metadata)>> {
    let expected: BTreeSet<_> = members
        .keys()
        .map(|name| root.join(name))
        .chain([root.join("index.json")])
        .collect();
    let mut expected_directories = BTreeSet::from([root.to_owned()]);
    for run in runs {
        let directory = root.join(&run.path);
        expected_directories.insert(directory.clone());
        expected_directories.insert(directory.join("inputs"));
        expected_directories.extend(
            run.params
                .keys()
                .map(|label| directory.join(format!("params-{label}"))),
        );
    }
    let mut files = BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    let mut directories = Vec::new();
    let (mut apparent, mut allocated) = (0_u64, 0_u64);
    while let Some(path) = stack.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if path.canonicalize()? != path
            || !metadata.is_dir()
            || !expected_directories.contains(&path)
        {
            return Err("lifecycle capture directory alias rejected".into());
        }
        apparent += metadata.len();
        allocated += metadata.blocks() * 512;
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let value = fs::symlink_metadata(entry.path())?;
            if value.is_dir() {
                stack.push(entry.path());
            } else if value.is_file() {
                if !expected.contains(&entry.path()) || !files.insert(entry.path()) {
                    return Err("unexpected lifecycle capture file".into());
                }
                apparent += value.len();
                allocated += value.blocks() * 512;
            } else {
                return Err("lifecycle capture special/symlink entry rejected".into());
            }
        }
        directories.push((path, metadata));
        if files.len() + directories.len() + stack.len() > 62000
            || apparent > 64 * MIB
            || allocated > 64 * MIB
        {
            return Err("lifecycle capture tree exceeds member/du bounds".into());
        }
    }
    if files != expected || directories.len() != expected_directories.len() {
        return Err("incomplete lifecycle capture tree".into());
    }
    Ok(directories)
}

fn validate_index(index: &Index, audit: &Audit) -> Result<()> {
    let p = &index.provenance;
    let (runner_arch, host) = match audit.host_architecture.as_str() {
        "x86_64" => ("X64", "x86_64-unknown-linux-gnu"),
        "aarch64" => ("ARM64", "aarch64-unknown-linux-gnu"),
        _ => return Err("unsupported original lifecycle architecture".into()),
    };
    if index.schema_version != 1
        || index.kind != "independent-original-planner-lifecycle-inputs"
        || index.status != "COMPLETE"
        || index.runs.len() != 26
        || index.inputs != 234
        || index.params_snapshots != 30
        || index.capture_limit_bytes != 64 * MIB
        || !sha(&index.receipt_sha256)
        || p.source_commit != audit.source_commit
        || p.source_sha256 != audit.source_sha256
        || p.source_sha256.len() != SOURCES.len()
        || SOURCES
            .iter()
            .any(|key| !p.source_sha256.get(*key).is_some_and(|v| sha(v)))
        || p.owner
            != format!(
                "{}:{}:{}",
                audit.origin_run_id, audit.origin_run_attempt, audit.origin_job
            )
        || p.environment.get("GITHUB_SHA") != Some(&p.source_commit)
        || p.environment.get("GITHUB_RUN_ID") != Some(&audit.origin_run_id)
        || p.environment.get("GITHUB_RUN_ATTEMPT") != Some(&audit.origin_run_attempt)
        || p.environment.get("GITHUB_JOB") != Some(&audit.origin_job)
        || p.environment.get("GITHUB_REPOSITORY").map(String::as_str)
            != Some("bin9208/openpilot-rust")
        || p.environment.get("RUNNER_OS").map(String::as_str) != Some("Linux")
        || p.environment.get("RUNNER_ARCH").map(String::as_str) != Some(runner_arch)
        || p.environment.get("NATIVE_TOOLS_HOST").map(String::as_str) != Some(host)
        || p.identities.len() != 7
        || p.sdk_before_proof.keys().any(|key| {
            ![
                "controller-source.txt",
                "controller-profile.txt",
                "controller.sha256",
                "rustc.txt",
                "sdk.before.sha256",
                "prepared_manifest",
            ]
            .contains(&key.as_str())
        })
        || p.admission.existing_bytes != 128 * MIB
        || p.admission.capture_bytes != 64 * MIB
        || p.admission.free_bytes < 192 * MIB
        || index.usage_before_index.payload_bytes > 64 * MIB
        || index.usage_before_index.apparent_bytes > 64 * MIB
        || index.usage_before_index.du_bytes > 64 * MIB
        || ![&p.python_argv, &p.python_executable, &p.source_native]
            .iter()
            .all(|v| Path::new(v).is_absolute())
        || p.python_version.is_empty()
    {
        return Err("invalid lifecycle source/owner/budget provenance".into());
    }
    for key in [
        "python_target",
        "binary",
        "params_binding",
        "original_ipc_binding",
        "artifact_manifest",
        "lateral_source_solver",
        "longitudinal_source_solver",
    ] {
        if !p.identities.contains_key(key) {
            return Err("missing original lifecycle binary/binding role".into());
        }
    }
    for key in [
        "controller-source.txt",
        "controller-profile.txt",
        "controller.sha256",
        "rustc.txt",
        "sdk.before.sha256",
    ] {
        if !p.sdk_before_proof.contains_key(key) {
            return Err("missing original lifecycle SDK proof".into());
        }
    }
    if p.identities
        .values()
        .chain(p.sdk_before_proof.values())
        .any(|v| !Path::new(&v.path).is_absolute() || v.bytes == 0 || !sha(&v.sha256))
    {
        return Err("invalid original lifecycle identity".into());
    }
    Ok(())
}

fn bind_original_invocation(run: &Run, provenance: &Provenance, receipt: &Path) -> Result<()> {
    let original = Path::new("/")
        .join(receipt)
        .parent()
        .ok_or("original case output")?
        .to_owned();
    let env = &run.environment;
    let root = PathBuf::from(env.get("PWD").ok_or("original PWD")?);
    if env.keys().any(|key| {
        ![
            "PARAMS_ROOT",
            "PWD",
            "LOGPRINT",
            "SIMULATION",
            "PYTHONDONTWRITEBYTECODE",
            "OPENBLAS_NUM_THREADS",
            "PYTHONUNBUFFERED",
        ]
        .contains(&key.as_str())
    }) || !root.is_absolute()
        || env.get("PARAMS_ROOT").map(PathBuf::from) != Some(original.join("params"))
        || env.get("LOGPRINT").map(String::as_str) != Some("info")
        || env.get("SIMULATION").map(String::as_str) != Some("0")
        || [
            "PYTHONDONTWRITEBYTECODE",
            "OPENBLAS_NUM_THREADS",
            "PYTHONUNBUFFERED",
        ]
        .iter()
        .any(|key| env.get(*key).map(String::as_str) != Some("1"))
    {
        return Err("original lifecycle invocation environment differs".into());
    }
    let argv = if run.mode == "source" {
        vec![
            provenance.python_argv.clone(),
            "-u".into(),
            "-P".into(),
            root.join("rust/tools/plannerd_daemon_source.py")
                .to_str()
                .ok_or("original source path")?
                .into(),
            "--binding".into(),
            provenance.identities["params_binding"].path.clone(),
            "--source-native".into(),
            provenance.source_native.clone(),
            "--log-root".into(),
            original
                .join("logs")
                .to_str()
                .ok_or("original log path")?
                .into(),
        ]
    } else {
        vec![
            provenance.identities["binary"].path.clone(),
            "--solver".into(),
            Path::new(&provenance.identities["artifact_manifest"].path)
                .parent()
                .and_then(|v| v.to_str())
                .ok_or("original solver path")?
                .into(),
        ]
    };
    if run.argv != argv {
        return Err("original lifecycle argv/source/binding role differs".into());
    }
    Ok(())
}

pub(super) fn validate_run(run: &Run, indexed: &IndexedRun, i: usize) -> Result<()> {
    let case = CASES.get(i / 2).ok_or("lifecycle run index")?;
    let mode = if i.is_multiple_of(2) {
        "source"
    } else {
        "rust"
    };
    let path = format!("{i:02}-{}-{mode}", case.name);
    let labels = if case.phase == "recover" {
        vec!["prelaunch", "recovery-before", "recovery-after"]
    } else {
        vec!["prelaunch"]
    };
    let directories: Vec<_> = if case.name == "settings-directories" {
        SETTINGS.to_vec()
    } else {
        Vec::new()
    };
    if indexed.case != case.name
        || indexed.mode != mode
        || indexed.epoch != 0
        || indexed.run_index != i
        || indexed.manifest.path != format!("{path}/manifest.json")
        || run.case != case.name
        || run.mode != mode
        || run.epoch != 0
        || run.run_index != i
        || run.path != path
        || run.phase != case.phase
        || run.signal != case.signal
        || run.car_params != case.car_params
        || run.directories != directories
        || run.frames != case.frames()
        || run.inputs.len() != 9 * case.frames()
        || run.params.len() != labels.len()
        || labels.iter().any(|key| !run.params.contains_key(*key))
        || run.prefix.is_empty()
        || run.argv.is_empty()
        || run.environment.len() != 7
        || !run.result.ok
        || run.result.case != case.name
        || run.result.mode != mode
        || run.result.returncode != Some(case.expected(mode))
        || run.result.expected_returncode != Some(case.expected(mode))
        || run.result.error.is_some()
        || run.result.publications.is_some() != matches!(case.phase, "ready" | "recover" | "float")
        || !sha(&run.result_receipt_sha256)
    {
        return Err("lifecycle original case/order/raw exit contract differs".into());
    }
    for (n, packet) in run.inputs.iter().enumerate() {
        if packet.frame != n / 9
            || packet.service != SEND_ORDER[n % 9]
            || packet.raw.bytes == 0
            || packet.raw.path != format!("{path}/inputs/{n:02}-{}.bin", packet.service)
        {
            return Err("lifecycle input service/frame/order differs".into());
        }
    }
    let names: BTreeSet<_> = run.param_names.iter().collect();
    if names.len() != run.param_names.len()
        || names.len() > 2048
        || !run.param_names.iter().any(|v| v == "CarParams")
    {
        return Err("invalid lifecycle Params key roster".into());
    }
    let mut observed_names = BTreeSet::new();
    let pre = run.params["prelaunch"]
        .entries
        .get("CarParams")
        .ok_or("missing launch CP state")?;
    let state_ok = match (case.phase, case.car_params, pre) {
        ("wait" | "recover", "missing", State::Absent { .. })
        | ("wait" | "recover", "directory", State::Directory { .. }) => true,
        ("wait", "empty", State::File { raw }) => raw.bytes == 0,
        ("ready" | "integer" | "float" | "malformed", _, State::File { raw }) => raw.bytes > 0,
        _ => false,
    };
    if !state_ok {
        return Err("observed launch CP fault state differs".into());
    }
    if case.phase == "recover"
        && (!matches!(
            run.params["recovery-before"].entries.get("CarParams"),
            Some(State::Directory { .. })
        ) || !matches!(run.params["recovery-after"].entries.get("CarParams"), Some(State::File { raw }) if raw.bytes > 0))
    {
        return Err("observed recovery CP transition differs".into());
    }
    if case.name == "settings-directories"
        && SETTINGS.iter().any(|key| {
            !matches!(
                run.params["prelaunch"].entries.get(*key),
                Some(State::Directory { .. })
            )
        })
    {
        return Err("observed setting directory faults differ".into());
    }
    for (label, snapshot) in &run.params {
        if !snapshot.monotonic.is_finite()
            || !snapshot
                .entries
                .keys()
                .collect::<BTreeSet<_>>()
                .is_subset(&names)
            || !snapshot.entries.contains_key("CarParams")
        {
            return Err("invalid lifecycle snapshot roster/time".into());
        }
        observed_names.extend(snapshot.entries.keys());
        for (key, state) in &snapshot.entries {
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err("invalid lifecycle Params name".into());
            }
            match state {
                State::File { raw } if raw.path != format!("{path}/params-{label}/{key}") => {
                    return Err("lifecycle Params role escaped snapshot".into());
                }
                _ => {}
            }
        }
    }
    if observed_names != names {
        return Err("lifecycle final Params names differ from observed union".into());
    }
    Ok(())
}
