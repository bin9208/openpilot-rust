//! 실제 writer의 index/네 manifest를 읽는다. 신뢰의 시작점은 부모가 감사 후 고정한 외부 proof SHA다.
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

pub const SERVICES: [&str; 10] = [
    "carControl",
    "carState",
    "controlsState",
    "liveParameters",
    "radarState",
    "liveTracks",
    "modelV2",
    "selfdriveState",
    "carrotMan",
    "livePose",
];
const SEND_ORDER: [&str; 9] = [
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
const SOURCES: [&str; 9] = [
    "rust/tools/plannerd_ipc.py",
    "rust/tools/plannerd_owner_fixtures.py",
    "openpilot/common/params_keys.h",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "openpilot/cereal/services.py",
    "openpilot/selfdrive/modeld/constants.py",
    "openpilot/cereal/messaging/__init__.py",
];
const RUNS: [(&str, usize); 4] = [("source", 0), ("source", 1), ("rust", 0), ("rust", 1)];
const MIB: u64 = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub service: String,
    pub raw: RawFile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub index: usize,
    pub packets: Vec<Packet>,
    pub live: RawFile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub kind: String,
    pub source_commit: String,
    pub source_sha256: BTreeMap<String, String>,
    pub capture_provenance: BTreeMap<String, String>,
    pub params: BTreeMap<String, RawFile>,
    pub car_params: RawFile,
    pub initial_live: RawFile,
    pub frames: Vec<Frame>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedRun {
    pub mode: String,
    pub epoch: usize,
    pub events: usize,
    pub params_files: usize,
    pub manifest: RawFile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub schema_version: u32,
    pub kind: String,
    pub status: String,
    pub source_commit: String,
    pub source_sha256: BTreeMap<String, String>,
    pub runs: Vec<IndexedRun>,
    pub events: usize,
    pub frames: usize,
    pub car_params_files: usize,
    pub actual_output_receipt_sha256: String,
}

/// 업로드 후 원문과 ZIP을 감사한 부모가 작성/고정한다. capture writer의 schema와 분리한다.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub schema_version: u32,
    pub kind: String,
    pub source_commit: String,
    pub source_sha256: BTreeMap<String, String>,
    pub origin_run_id: String,
    pub origin_run_attempt: String,
    pub origin_job: String,
    pub host_architecture: String,
    /// capture private ZIP과 원본 receipt functional ZIP을 별도로 고정한다.
    pub artifacts: BTreeMap<String, Artifact>,
    /// 각각 artifacts[capture]/artifacts[receipt]의 extraction root에 상대적인 실제 ZIP member다.
    pub index: RawFile,
    pub receipt: RawFile,
    /// role -> 원래 receipt의 절대 경로 key와 SHA. 현재 경로로 원문 key를 덮어쓰지 않는다.
    pub identities: BTreeMap<String, OriginalIdentity>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: u64,
    pub bytes: u64,
    pub sha256: String,
    /// audit bundle root에 상대적인 보존 ZIP과 안전 추출 디렉터리다.
    pub archive: String,
    pub extracted: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalIdentity {
    pub captured_path: String,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    identities: BTreeMap<String, String>,
    runs: Vec<Summary>,
    comparisons: Vec<Comparison>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Summary {
    returncode: i32,
    messages: usize,
    valid_counts: BTreeMap<String, usize>,
    triggers: BTreeMap<String, usize>,
    fast_overlays: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    run: usize,
    differences: Vec<serde_json::Value>,
}

fn hex(value: &str, count: usize) -> bool {
    value.len() == count
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn positive_id(value: &str) -> bool {
    !value.starts_with('0') && value.parse::<u64>().is_ok_and(|n| n > 0)
}
fn member(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("invalid original capture member path".into());
    }
    let path = root.join(relative);
    if path.canonicalize()? != path {
        return Err("original capture member contains a symlink".into());
    }
    Ok(path)
}
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    (
        a.dev(),
        a.ino(),
        a.mode(),
        a.len(),
        a.mtime(),
        a.mtime_nsec(),
        a.ctime(),
        a.ctime_nsec(),
    ) == (
        b.dev(),
        b.ino(),
        b.mode(),
        b.len(),
        b.mtime(),
        b.mtime_nsec(),
        b.ctime(),
        b.ctime_nsec(),
    )
}
/// archive는 streaming hash만 계산해 RAM에 ZIP 전체를 올리지 않는다.
fn pinned(path: &Path, sha: &str, bytes: Option<u64>, limit: u64, retain: bool) -> Result<Vec<u8>> {
    if !hex(sha, 64) || !path.is_absolute() || path.canonicalize()? != path {
        return Err("canonical capture path and pinned SHA256 required".into());
    }
    let mut file = File::from(open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit || bytes.is_some_and(|n| n != before.len()) {
        return Err("bounded regular capture file required".into());
    }
    let mut result = Vec::new();
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > limit {
            return Err("capture file grew beyond bound".into());
        }
        hash.update(&buffer[..n]);
        if retain {
            result.extend_from_slice(&buffer[..n]);
        }
    }
    if count != before.len()
        || format!("{:x}", hash.finalize()) != sha
        || !same_file(&before, &file.metadata()?)
        || !same_file(&before, &fs::symlink_metadata(path)?)
        || path.canonicalize()? != path
    {
        return Err("original capture identity/hash changed".into());
    }
    Ok(result)
}
fn read(root: &Path, raw: &RawFile, limit: u64) -> Result<Vec<u8>> {
    pinned(
        &member(root, &raw.path)?,
        &raw.sha256,
        Some(raw.bytes),
        limit,
        true,
    )
}

pub struct Corpus {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub manifest_sha256: String,
    manifest_bytes: u64,
    total: u64,
}
impl Corpus {
    fn open(root: PathBuf, indexed: &IndexedRun, index: &Index, audit: &Audit) -> Result<Self> {
        let raw = read(
            &root,
            &RawFile {
                path: "manifest.json".into(),
                ..indexed.manifest.clone()
            },
            MIB,
        )?;
        let manifest: Manifest = serde_json::from_slice(&raw)?;
        let provenance = &manifest.capture_provenance;
        if manifest.schema_version != 1
            || manifest.kind != "independent-original-planner-ipc-inputs"
            || manifest.frames.len() != 40
            || manifest.source_commit != index.source_commit
            || manifest.source_sha256 != index.source_sha256
            || manifest.params.len() > 2048
            || indexed.params_files != manifest.params.len()
            || provenance.len() != 15
        {
            return Err("original per-run input manifest differs from index".into());
        }
        let epoch = indexed.epoch.to_string();
        for (key, expected) in [
            ("origin_run_id", audit.origin_run_id.as_str()),
            ("origin_run_attempt", audit.origin_run_attempt.as_str()),
            ("origin_job", audit.origin_job.as_str()),
            ("host_architecture", audit.host_architecture.as_str()),
            ("mode", indexed.mode.as_str()),
            ("epoch", epoch.as_str()),
            ("input_transport", "actual-original-cpp-pubmaster"),
            ("params_snapshot", "before-daemon-launch"),
        ] {
            if provenance.get(key).map(String::as_str) != Some(expected) {
                return Err(format!("original capture provenance differs: {key}").into());
            }
        }
        for key in [
            "python_executable",
            "python_executable_canonical",
            "python_version",
            "numpy_version",
            "openpilot_prefix",
            "daemon_command_json",
        ] {
            if provenance.get(key).is_none_or(|s| s.is_empty()) {
                return Err("original capture process provenance incomplete".into());
            }
        }
        if !provenance
            .get("python_executable_sha256")
            .is_some_and(|s| hex(s, 64))
            || provenance.contains_key("origin_artifact_id")
        {
            return Err("writer Python identity or external artifact provenance differs".into());
        }
        let command: Vec<String> = serde_json::from_str(&provenance["daemon_command_json"])?;
        let command_matches = match indexed.mode.as_str() {
            "source" => {
                command.len() == 7
                    && command[0] == provenance["python_executable"]
                    && command[1] == "-P"
                    && command[2] == "rust/tools/plannerd_daemon_source.py"
                    && command[3] == "--binding"
                    && command[4] == audit.identities["params_binding"].captured_path
                    && command[5] == "--source-native"
                    && Path::new(&command[6]).is_absolute()
            }
            "rust" => {
                command.len() == 3
                    && command[0] == audit.identities["binary"].captured_path
                    && command[1] == "--solver"
                    && Path::new(&command[2]).join("manifest.json")
                        == Path::new(&audit.identities["solver_manifest"].captured_path)
            }
            _ => false,
        };
        if !command_matches {
            return Err(
                "original daemon argv differs from audited binding/solver identities".into(),
            );
        }
        let mut corpus = Self {
            root,
            manifest,
            manifest_sha256: indexed.manifest.sha256.clone(),
            manifest_bytes: indexed.manifest.bytes,
            total: indexed.manifest.bytes,
        };
        let mut total = corpus.total;
        let mut admit = |raw: &RawFile, path: &str, bound| -> Result<()> {
            if raw.path != path {
                return Err("original per-publication file path/order differs".into());
            }
            total = total
                .checked_add(raw.bytes)
                .ok_or("capture size overflow")?;
            if total > 64 * MIB {
                return Err("original run capture exceeds 64 MiB".into());
            }
            read(&corpus.root, raw, bound)?;
            Ok(())
        };
        admit(&corpus.manifest.car_params, "CarParams.pending.bin", MIB)?;
        admit(
            &corpus.manifest.initial_live,
            "events/0000-liveTracks.bin",
            MIB,
        )?;
        let mut params_total = 0_u64;
        for (name, raw) in &corpus.manifest.params {
            if name.is_empty()
                || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || name == "CarParams"
                || name == "CarParams.pending"
            {
                return Err("invalid initial Params snapshot key".into());
            }
            params_total = params_total
                .checked_add(raw.bytes)
                .ok_or("Params size overflow")?;
            if params_total > 4 * MIB {
                return Err("initial Params snapshot exceeds 4 MiB".into());
            }
            admit(raw, &format!("params/{name}"), 64 * 1024)?;
        }
        for (i, frame) in corpus.manifest.frames.iter().enumerate() {
            if frame.index != i || frame.packets.len() != 9 {
                return Err("original frame count/index differs".into());
            }
            for (p, (packet, service)) in frame.packets.iter().zip(SEND_ORDER).enumerate() {
                if packet.service != service {
                    return Err("original publication service order differs".into());
                }
                admit(
                    &packet.raw,
                    &format!("events/{:04}-{service}.bin", 1 + i * 10 + p),
                    MIB,
                )?;
            }
            admit(
                &frame.live,
                &format!("events/{:04}-liveTracks.bin", 10 + i * 10),
                MIB,
            )?;
        }
        for (name, expected) in [
            ("EnableRadarTracks", b"1".as_slice()),
            ("MyDrivingModeAuto", b"0".as_slice()),
        ] {
            if corpus.read(
                corpus
                    .manifest
                    .params
                    .get(name)
                    .ok_or("required original Params missing")?,
            )? != expected
            {
                return Err("original Params policy value differs".into());
            }
        }
        corpus.total = total;
        Ok(corpus)
    }
    pub fn path(&self, raw: &RawFile) -> Result<PathBuf> {
        member(&self.root, &raw.path)
    }
    pub fn read(&self, raw: &RawFile) -> Result<Vec<u8>> {
        read(&self.root, raw, MIB)
    }
    pub fn verify(&self) -> Result<()> {
        read(
            &self.root,
            &RawFile {
                path: "manifest.json".into(),
                bytes: self.manifest_bytes,
                sha256: self.manifest_sha256.clone(),
            },
            MIB,
        )?;
        self.read(&self.manifest.car_params)?;
        self.read(&self.manifest.initial_live)?;
        for raw in self.manifest.params.values() {
            self.read(raw)?;
        }
        for frame in &self.manifest.frames {
            for packet in &frame.packets {
                self.read(&packet.raw)?;
            }
            self.read(&frame.live)?;
        }
        Ok(())
    }
}

pub struct Capture {
    audit: Audit,
    audit_sha256: String,
    index: Index,
    root: PathBuf,
    audit_path: PathBuf,
    cases: Vec<Corpus>,
}
impl Capture {
    pub fn audit(&self) -> &Audit {
        &self.audit
    }
    pub fn audit_sha256(&self) -> &str {
        &self.audit_sha256
    }
    pub fn index(&self) -> &Index {
        &self.index
    }
    pub fn open(root: &Path, audit_path: &Path, audit_sha256: &str) -> Result<Self> {
        if !root.is_absolute()
            || root.canonicalize()? != root
            || !fs::symlink_metadata(root)?.is_dir()
        {
            return Err("canonical audited bundle root required".into());
        }
        let audit: Audit =
            serde_json::from_slice(&pinned(audit_path, audit_sha256, None, MIB, true)?)?;
        if audit.schema_version != 1
            || audit.kind != "audited-original-planner-ipc-capture"
            || !hex(&audit.source_commit, 40)
            || !positive_id(&audit.origin_run_id)
            || !positive_id(&audit.origin_run_attempt)
            || audit.origin_job.is_empty()
            || !["x86_64", "aarch64"].contains(&audit.host_architecture.as_str())
            || audit.source_sha256.len() != SOURCES.len()
            || SOURCES
                .iter()
                .any(|s| !audit.source_sha256.get(*s).is_some_and(|v| hex(v, 64)))
        {
            return Err("independent externally audited capture provenance required".into());
        }
        if audit.artifacts.len() != 2
            || !audit.artifacts.contains_key("capture")
            || !audit.artifacts.contains_key("receipt")
            || audit
                .artifacts
                .values()
                .map(|a| a.id)
                .collect::<BTreeSet<_>>()
                .len()
                != 2
            || audit
                .artifacts
                .values()
                .map(|a| &a.archive)
                .collect::<BTreeSet<_>>()
                .len()
                != 2
            || audit
                .artifacts
                .values()
                .map(|a| &a.extracted)
                .collect::<BTreeSet<_>>()
                .len()
                != 2
        {
            return Err("separate audited capture/receipt artifacts required".into());
        }
        for artifact in audit.artifacts.values() {
            if artifact.id == 0
                || artifact.bytes == 0
                || !fs::symlink_metadata(member(root, &artifact.extracted)?)?.is_dir()
            {
                return Err("audited artifact ID, size and extraction directory required".into());
            }
            pinned(
                &member(root, &artifact.archive)?,
                &artifact.sha256,
                Some(artifact.bytes),
                1024 * MIB,
                false,
            )?;
        }
        let inputs_root = member(root, &audit.artifacts["capture"].extracted)?;
        let receipt_root = member(root, &audit.artifacts["receipt"].extracted)?;
        let index: Index = serde_json::from_slice(&read(&inputs_root, &audit.index, MIB)?)?;
        if index.schema_version != 1
            || index.kind != "independent-original-planner-ipc-run-index"
            || index.status != "COMPLETE"
            || index.source_commit != audit.source_commit
            || index.source_sha256 != audit.source_sha256
            || index.runs.len() != 4
            || index.events != 1604
            || index.frames != 160
            || index.car_params_files != 4
            || index.actual_output_receipt_sha256 != audit.receipt.sha256
        {
            return Err("complete original four-run index and receipt pin required".into());
        }
        let receipt: Receipt = serde_json::from_slice(&read(&receipt_root, &audit.receipt, MIB)?)?;
        let roles = ["binary", "params_binding", "cpp_binding", "solver_manifest"];
        let declared = audit
            .identities
            .values()
            .map(|v| (v.captured_path.clone(), v.sha256.clone()))
            .collect::<BTreeMap<_, _>>();
        if audit.identities.len() != 4
            || declared.len() != 4
            || declared != receipt.identities
            || roles.iter().any(|role| {
                audit.identities.get(*role).is_none_or(|v| {
                    !Path::new(&v.captured_path).is_absolute() || !hex(&v.sha256, 64)
                })
            })
        {
            return Err("audited original binary/binding/solver receipt identities differ".into());
        }
        if receipt.runs.len() != 4
            || receipt.comparisons.len() != 3
            || receipt
                .comparisons
                .iter()
                .enumerate()
                .any(|(i, row)| row.run != i + 1 || !row.differences.is_empty())
            || receipt.runs.iter().any(|row| {
                row.returncode != -15
                    || row.messages == 0
                    || row.fast_overlays == 0
                    || row.valid_counts.len() != 3
                    || row.triggers.len() != 2
                    || ["longitudinalPlan", "lateralPlan", "driverAssistance"]
                        .iter()
                        .any(|s| row.valid_counts.get(*s).is_none_or(|n| *n == 0))
                    || ["modelV2", "liveTracks"]
                        .iter()
                        .any(|s| row.triggers.get(*s).is_none_or(|n| *n == 0))
            })
        {
            return Err("original actual IPC output comparison/coverage did not pass".into());
        }
        let index_path = member(&inputs_root, &audit.index.path)?;
        if index_path.file_name().and_then(|s| s.to_str()) != Some("index.json") {
            return Err("original root index.json required".into());
        }
        let capture_root = index_path.parent().ok_or("missing original capture root")?;
        let mut cases = Vec::new();
        let mut total = audit.index.bytes;
        for (entry, (mode, epoch)) in index.runs.iter().zip(RUNS) {
            if entry.mode != mode
                || entry.epoch != epoch
                || entry.events != 401
                || entry.manifest.path != format!("{mode}-{epoch}/manifest.json")
            {
                return Err("original mode/epoch index order or event count differs".into());
            }
            let path = member(capture_root, &entry.manifest.path)?;
            let case = Corpus::open(
                path.parent()
                    .ok_or("missing original case root")?
                    .to_owned(),
                entry,
                &index,
                &audit,
            )?;
            total = total
                .checked_add(case.total)
                .ok_or("capture size overflow")?;
            if total > 256 * MIB {
                return Err("original four-run capture exceeds 256 MiB".into());
            }
            cases.push(case);
        }
        Ok(Self {
            audit,
            audit_sha256: audit_sha256.into(),
            index,
            root: root.into(),
            audit_path: audit_path.into(),
            cases,
        })
    }
    pub fn select(&self, mode: &str, epoch: usize) -> Result<&Corpus> {
        let i = RUNS
            .iter()
            .position(|entry| *entry == (mode, epoch))
            .ok_or("unknown original capture mode/epoch")?;
        self.cases
            .get(i)
            .ok_or_else(|| "missing original mode/epoch corpus".into())
    }
    /// 역사 producer source를 현재 Python 파일과 혼동하지 않는다. 현재 decoder domain만 고정한다.
    pub fn verify_schema(&self, source_root: &Path) -> Result<()> {
        if self.audit.host_architecture != std::env::consts::ARCH {
            return Err("capture host domain differs from native run".into());
        }
        for name in [
            "openpilot/cereal/log.capnp",
            "openpilot/cereal/custom.capnp",
            "opendbc_repo/opendbc/car/car.capnp",
            "openpilot/cereal/services.py",
        ] {
            pinned(
                &source_root.join(name),
                &self.audit.source_sha256[name],
                None,
                16 * MIB,
                false,
            )?;
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<()> {
        pinned(&self.audit_path, &self.audit_sha256, None, MIB, false)?;
        for artifact in self.audit.artifacts.values() {
            pinned(
                &member(&self.root, &artifact.archive)?,
                &artifact.sha256,
                Some(artifact.bytes),
                1024 * MIB,
                false,
            )?;
        }
        read(
            &member(&self.root, &self.audit.artifacts["capture"].extracted)?,
            &self.audit.index,
            MIB,
        )?;
        read(
            &member(&self.root, &self.audit.artifacts["receipt"].extracted)?,
            &self.audit.receipt,
            MIB,
        )?;
        for case in &self.cases {
            case.verify()?;
        }
        Ok(())
    }
}
