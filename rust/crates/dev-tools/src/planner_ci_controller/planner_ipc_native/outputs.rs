//! aa79 원래 Python case receipt와 raw121×4를 읽는다. 후보로 expected를 만들지 않는다.
use super::{corpus::Capture, corpus::RawFile, decode, OUTPUTS};
use crate::seal::Result;
use rustix::fs::{open, Mode, OFlags};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const MIB: u64 = 1024 * 1024;
const CASES: [(&str, usize); 4] = [("source", 0), ("source", 1), ("rust", 0), ("rust", 1)];

pub(super) fn audit_bytes(architecture: &str) -> Result<&'static [u8]> {
    match architecture {
        "x86_64" => Ok(include_bytes!(
            "../../../tests/fixtures/planner-ipc-original-outputs/x86_64.json"
        )),
        "aarch64" => Ok(include_bytes!(
            "../../../tests/fixtures/planner-ipc-original-outputs/aarch64.json"
        )),
        _ => Err("unsupported original Planner output architecture".into()),
    }
}

fn relative(path: &str) -> bool {
    !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn bounded(raw: &RawFile) -> bool {
    relative(&raw.path)
        && (1..=MIB).contains(&raw.bytes)
        && raw.sha256.len() == 64
        && raw
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputAudit {
    schema_version: u32,
    kind: String,
    input_audit_sha256: String,
    receipt_artifact_id: u64,
    pub(super) runs: Vec<Run>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Run {
    mode: String,
    epoch: usize,
    pub(super) receipt: RawFile,
    pub(super) publications: Vec<RawFile>,
}
impl OutputAudit {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MIB as usize {
            return Err("original Planner output audit exceeds 1 MiB".into());
        }
        let audit: Self = serde_json::from_slice(bytes)?;
        if audit.schema_version != 1
            || audit.kind != "audited-original-planner-ipc-outputs"
            || audit.receipt_artifact_id == 0
            || audit.input_audit_sha256.len() != 64
            || !audit
                .input_audit_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || audit.runs.len() != 4
        {
            return Err("complete original Planner output audit required".into());
        }
        let mut total = 0_u64;
        let mut paths = BTreeSet::new();
        for (run, (mode, epoch)) in audit.runs.iter().zip(CASES) {
            if run.mode != mode
                || run.epoch != epoch
                || !bounded(&run.receipt)
                || !run
                    .receipt
                    .path
                    .ends_with(&format!("/{mode}-{epoch}/receipt.json"))
                || run.publications.len() != 121
            {
                return Err(
                    "original output mode/epoch, receipt or 121 publications differ".into(),
                );
            }
            let directory = Path::new(&run.receipt.path)
                .parent()
                .ok_or("missing original output case directory")?;
            let mut indices = BTreeSet::new();
            for raw in std::iter::once(&run.receipt).chain(&run.publications) {
                if !bounded(raw) || !paths.insert(raw.path.clone()) {
                    return Err("invalid or duplicate original output member".into());
                }
                total = total.checked_add(raw.bytes).ok_or("output size overflow")?;
                if total > 8 * MIB {
                    return Err("original Planner outputs exceed 8 MiB".into());
                }
            }
            for raw in &run.publications {
                let path = Path::new(&raw.path);
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("invalid original output filename")?;
                let (index, service) = name
                    .split_once('-')
                    .ok_or("missing original output publication index")?;
                if path.parent() != Some(directory)
                    || index.len() != 4
                    || !index.bytes().all(|b| b.is_ascii_digit())
                    || index.parse::<usize>()? >= 121
                    || !indices.insert(index.to_owned())
                    || !service
                        .strip_suffix(".bin")
                        .is_some_and(|s| OUTPUTS.contains(&s))
                {
                    return Err("original output publication path/order differs".into());
                }
            }
        }
        Ok(audit)
    }
    pub(super) fn bind(&self, capture: &Capture) -> Result<()> {
        self.bind_input(
            capture.audit_sha256(),
            capture.audit().artifacts["receipt"].id,
            &capture.audit().receipt.path,
        )
    }
    pub(super) fn bind_input(&self, sha: &str, artifact: u64, summary: &str) -> Result<()> {
        if self.input_audit_sha256 != sha || self.receipt_artifact_id != artifact {
            return Err(
                "original output audit differs from authenticated input/receipt artifact".into(),
            );
        }
        let directory = Path::new(summary)
            .parent()
            .ok_or("missing original IPC summary directory")?;
        for (run, (mode, epoch)) in self.runs.iter().zip(CASES) {
            if Path::new(&run.receipt.path)
                != directory.join(format!("{mode}-{epoch}/receipt.json"))
            {
                return Err("original output receipt escaped the authenticated cohort".into());
            }
        }
        Ok(())
    }
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
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

/// 열린 FD/path를 읽기 전후 대조하고, 반환 후에도 처음 관측한 identity를 유지한다.
pub(super) struct OriginalFile {
    root: PathBuf,
    raw: RawFile,
    before: Metadata,
}
impl OriginalFile {
    pub(super) fn read(root: &Path, raw: &RawFile) -> Result<(Vec<u8>, Self)> {
        if !bounded(raw) || !root.is_absolute() || root.canonicalize()? != root {
            return Err("bounded canonical original output member required".into());
        }
        let path = root.join(&raw.path);
        if path.canonicalize()? != path {
            return Err("original output contains a symlink".into());
        }
        let mut file = File::from(open(
            &path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let before = file.metadata()?;
        if !before.is_file() || before.len() != raw.bytes {
            return Err("original output is not the declared bounded regular file".into());
        }
        let mut bytes = Vec::new();
        file.by_ref().take(raw.bytes + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != raw.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != raw.sha256
            || !same_file(&before, &file.metadata()?)
            || !same_file(&before, &fs::symlink_metadata(&path)?)
            || path.canonicalize()? != path
        {
            return Err("original output bytes or FD/path identity changed".into());
        }
        Ok((
            bytes,
            Self {
                root: root.into(),
                raw: raw.clone(),
                before,
            },
        ))
    }
    pub(super) fn verify(&self) -> Result<()> {
        let (_, current) = Self::read(&self.root, &self.raw)?;
        if !same_file(&self.before, &current.before) {
            return Err("original output reappeared after admission".into());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseReceipt {
    command: Vec<String>,
    returncode: i32,
    records: Vec<decode::Record>,
    timing: Vec<Value>,
    valid_counts: Value,
    triggers: Value,
    fast_overlays: usize,
}

pub(super) fn verify_packet(raw: &[u8], expected: &decode::Record) -> Result<()> {
    let decoded = decode::packet(raw, &expected.service, expected.path.clone())?;
    if decoded.timestamp != expected.timestamp
        || !decode::differences(std::slice::from_ref(expected), &[decoded]).is_empty()
    {
        return Err("original raw packet differs from the original Python policy record".into());
    }
    Ok(())
}

pub(super) struct OriginalOutputs {
    audit: OutputAudit,
    audit_sha256: String,
    cases: Vec<Vec<decode::Record>>,
    files: Vec<OriginalFile>,
}
impl OriginalOutputs {
    pub(super) fn open(root: &Path, capture: &Capture) -> Result<Self> {
        if capture.audit().host_architecture != std::env::consts::ARCH {
            return Err("original output host differs from the native host".into());
        }
        let bytes = audit_bytes(&capture.audit().host_architecture)?;
        let audit = OutputAudit::parse(bytes)?;
        audit.bind(capture)?;
        let total = audit
            .runs
            .iter()
            .flat_map(|r| std::iter::once(&r.receipt).chain(&r.publications))
            .map(|raw| raw.bytes)
            .sum::<u64>()
            + bytes.len() as u64
            + capture.audit().receipt.bytes;
        if total > 8 * MIB {
            return Err("original output proof including metadata exceeds 8 MiB".into());
        }
        let receipt_root = root.join(&capture.audit().artifacts["receipt"].extracted);
        let (summary_bytes, summary_file) =
            OriginalFile::read(&receipt_root, &capture.audit().receipt)?;
        let summary: Value = serde_json::from_slice(&summary_bytes)?;
        let mut files = vec![summary_file];
        let mut cases = Vec::new();
        for (index, run) in audit.runs.iter().enumerate() {
            let (bytes, guard) = OriginalFile::read(&receipt_root, &run.receipt)?;
            files.push(guard);
            let case: CaseReceipt = serde_json::from_slice(&bytes)?;
            let command: Vec<String> = serde_json::from_str(
                &capture
                    .select(&run.mode, run.epoch)?
                    .manifest
                    .capture_provenance["daemon_command_json"],
            )?;
            let observed = json!({"returncode":case.returncode,"messages":case.records.len(),
                "valid_counts":case.valid_counts,"triggers":case.triggers,"fast_overlays":case.fast_overlays});
            if case.returncode != -15
                || case.command != command
                || case.records.len() != 121
                || case.timing.len() != 40
                || summary["runs"][index] != observed
                || case
                    .records
                    .windows(2)
                    .any(|r| r[0].timestamp > r[1].timestamp)
            {
                return Err(
                    "original case receipt/command/coverage differs from its authenticated summary"
                        .into(),
                );
            }
            for (expected, raw) in case.records.iter().zip(&run.publications) {
                if expected.path.strip_prefix('/') != Some(raw.path.as_str())
                    || !OUTPUTS.contains(&expected.service.as_str())
                    || !raw.path.ends_with(&format!("-{}.bin", expected.service))
                {
                    return Err("original Python record/raw publication binding differs".into());
                }
                let (bytes, guard) = OriginalFile::read(&receipt_root, raw)?;
                verify_packet(&bytes, expected)?;
                files.push(guard);
            }
            cases.push(case.records);
        }
        Ok(Self {
            audit,
            audit_sha256: format!("{:x}", Sha256::digest(bytes)),
            cases,
            files,
        })
    }
    pub(super) fn provenance(&self) -> Value {
        json!({"outputs_audit_sha256":self.audit_sha256,
            "input_audit_sha256":self.audit.input_audit_sha256,
            "receipt_artifact_id":self.audit.receipt_artifact_id,
            "cases":4,"original_packets_decoded":484,
            "expected_source":"independent original Python case receipts; no candidate generation"})
    }
    pub(super) fn compare(&self, current: &[Vec<decode::Record>]) -> Result<Value> {
        if current.len() != 4 {
            return Err(
                "exactly four current IPC cases required for original output comparison".into(),
            );
        }
        let rows = CASES.iter().enumerate().map(|(i, (mode, epoch))| {
            json!({"mode":mode,"epoch":epoch,"original_records":self.cases[i].len(),
                "current_records":current[i].len(),"differences":decode::differences(&self.cases[i], &current[i])})
        }).collect::<Vec<_>>();
        let passed = rows
            .iter()
            .all(|r| r["differences"].as_array().is_some_and(Vec::is_empty));
        Ok(
            json!({"scope":"original 121x4 raw/Python policy and current IPC publications only",
            "provenance":self.provenance(),"runs":rows,"passed":passed,
            "full_runtime_acceptance":false,"retirement_credit":0}),
        )
    }
    pub(super) fn verify(&self) -> Result<()> {
        for file in &self.files {
            file.verify()?;
        }
        Ok(())
    }
}
