//! 원본841의 gap857 관측을 기존 typed trace에 전달하는 단일 native leaf.
//! 원본 Python을 호출하거나 입력·expected를 새로 계산하지 않는다. 등록은 부모가 맡는다.
use crate::{
    qa_process::{CaptureError, Captured, CommandSpec, NativeRunner},
    seal::Result,
};
use rustix::{
    fs::{open, Mode, OFlags},
    process::{getpgrp, getpid, getuid},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

const MIB: u64 = 1024 * 1024;
const STAGE_LIMIT: u64 = 16 * MIB;
const SOURCE: &str = "841ee8a4e1357e7a0db4193cf7944e23c31ee0fb";
const X86_AUDIT: &[u8] =
    include_bytes!("../../tests/fixtures/planner-gap-original-audit/x86_64.json");
const ARM_AUDIT: &[u8] =
    include_bytes!("../../tests/fixtures/planner-gap-original-audit/aarch64.json");

pub(crate) struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub corpus: PathBuf,
    pub audit: PathBuf,
    pub output: PathBuf,
    pub timeout: Duration,
    pub growth_bytes: u64,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    actions: usize,
    owners: usize,
    positive_plans: usize,
    positive_credit: usize,
    mismatches: usize,
    first_mismatches: Vec<Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    bytes: u64,
    member: String,
    path: String,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    bytes: u64,
    id: u64,
    path: String,
    role: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audit {
    artifact: Artifact,
    expected: RawFile,
    host_architecture: String,
    input: RawFile,
    kind: String,
    origin_job: String,
    origin_run_attempt: String,
    origin_run_id: String,
    receipt: RawFile,
    receipt_contract: Receipt,
    schema_version: u32,
    source_commit: String,
    source_sha256: BTreeMap<String, String>,
}

fn qualified_audit(host: &str) -> Result<&'static [u8]> {
    match host {
        "x86_64" => Ok(X86_AUDIT),
        "aarch64" => Ok(ARM_AUDIT),
        _ => Err("unsupported gap native host".into()),
    }
}

fn parse_audit(bytes: &[u8], host: &str) -> Result<Audit> {
    // JSON 내용뿐 아니라 Hypatia가 실제 원본에서 작성한 metadata bytes를 고정한다.
    if bytes != qualified_audit(host)? {
        return Err("gap audit differs from qualified original841 metadata".into());
    }
    let audit: Audit = serde_json::from_slice(bytes)?;
    if audit.source_commit != SOURCE
        || audit.schema_version != 1
        || audit.kind != "audited-original-planner-gap-inputs"
        || audit.host_architecture != host
        || audit.origin_job != "planner-runtime"
        || audit.origin_run_attempt != "1"
        || audit.origin_run_id.is_empty()
        || audit.artifact.role != "lifecycle-functional"
        || audit.source_sha256.len() != 1
        || audit
            .source_sha256
            .get("rust/tools/plannerd_gap_source.py")
            .map(String::as_str)
            != Some("b047d9fcf33e42795e4b3bae13fa8f7fde17a39bbd189c15d30f4aecc27f46ba")
    {
        return Err("gap original source/host provenance differs".into());
    }
    Ok(audit)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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

fn owned_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if path.canonicalize()? != path
        || !metadata.is_dir()
        || metadata.uid() != getuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err("gap requires a canonical owned mode700 directory".into());
    }
    Ok(())
}

struct HeldFile {
    path: PathBuf,
    before: Metadata,
    sha256: String,
}

impl HeldFile {
    fn open(path: &Path, bytes: u64, sha256: &str, retain: bool) -> Result<(Vec<u8>, Self)> {
        if path.canonicalize()? != path {
            return Err("gap file symlink/alias rejected".into());
        }
        let mut file = File::from(open(
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let before = file.metadata()?;
        if !before.is_file()
            || before.len() != bytes
            || before.uid() != getuid().as_raw()
            || before.permissions().mode() & 0o022 != 0
        {
            return Err("gap regular owned file size/mode differs".into());
        }
        let mut digest = Sha256::new();
        let mut collected = Vec::new();
        let mut count = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > bytes {
                return Err("gap file grew while reading".into());
            }
            digest.update(&buffer[..n]);
            if retain {
                collected.extend_from_slice(&buffer[..n]);
            }
        }
        if count != bytes
            || format!("{:x}", digest.finalize()) != sha256
            || !identity(&before, &file.metadata()?)
            || !identity(&before, &fs::symlink_metadata(path)?)
        {
            return Err("gap file SHA/FD/path identity differs".into());
        }
        Ok((
            collected,
            Self {
                path: path.into(),
                before,
                sha256: sha256.into(),
            },
        ))
    }

    fn verify(&self) -> Result<()> {
        let (_, current) = Self::open(&self.path, self.before.len(), &self.sha256, false)?;
        if !identity(&self.before, &current.before) {
            return Err("gap held file was replaced".into());
        }
        Ok(())
    }
}

fn stage_roster(root: &Path) -> Result<()> {
    owned_directory(root)?;
    let expected: BTreeSet<_> = ["input.json", "expected.json", "receipt.json", "audit.json"]
        .map(std::ffi::OsString::from)
        .into_iter()
        .collect();
    let mut names = BTreeSet::new();
    let mut bytes = 0_u64;
    let mut allocated = fs::symlink_metadata(root)?.blocks().saturating_mul(512);
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !expected.contains(&entry.file_name()) {
            return Err("unexpected gap stage entry".into());
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_file()
            || metadata.uid() != getuid().as_raw()
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err("gap stage requires exclusive regular mode600 files".into());
        }
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("gap stage size overflow")?;
        allocated = allocated
            .checked_add(metadata.blocks().saturating_mul(512))
            .ok_or("gap stage allocated size overflow")?;
        if bytes > STAGE_LIMIT || allocated > STAGE_LIMIT {
            return Err("gap stage exceeds the original16MiB admission".into());
        }
        names.insert(entry.file_name());
    }
    if names != expected {
        return Err("gap stage roster/16MiB bound differs".into());
    }
    Ok(())
}

struct Corpus {
    root: PathBuf,
    root_before: Metadata,
    archive_root: PathBuf,
    archive_root_before: Metadata,
    input: Vec<u8>,
    expected_bytes: Vec<u8>,
    expected: Vec<Value>,
    audit: Audit,
    held: Vec<HeldFile>,
}

impl Corpus {
    fn open(root: &Path, audit_path: &Path, archive_root: &Path, host: &str) -> Result<Self> {
        stage_roster(root)?;
        owned_directory(archive_root)?;
        if audit_path != root.join("audit.json") {
            return Err("gap audit must be the fixed stage sibling".into());
        }
        let pinned = qualified_audit(host)?;
        let (bytes, held_audit) =
            HeldFile::open(audit_path, pinned.len() as u64, &hash(pinned), true)?;
        let audit = parse_audit(&bytes, host)?;
        let mut components = Path::new(&audit.artifact.path).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err("gap original archive must stay in the same-job lifecycle root".into());
        }
        let (_, archive) = HeldFile::open(
            &archive_root.join(&audit.artifact.path),
            audit.artifact.bytes,
            &audit.artifact.sha256,
            false,
        )?;
        let mut held = vec![held_audit, archive];
        let mut read = |raw: &RawFile, name: &str| -> Result<Vec<u8>> {
            if raw.path != name || raw.bytes > STAGE_LIMIT {
                return Err("gap fixed input descriptor differs".into());
            }
            let (bytes, file) = HeldFile::open(&root.join(name), raw.bytes, &raw.sha256, true)?;
            held.push(file);
            Ok(bytes)
        };
        let input = read(&audit.input, "input.json")?;
        let expected_bytes = read(&audit.expected, "expected.json")?;
        let receipt: Receipt = serde_json::from_slice(&read(&audit.receipt, "receipt.json")?)?;
        let actions: Vec<Value> = serde_json::from_slice(&input)?;
        let expected: Vec<Value> = serde_json::from_slice(&expected_bytes)?;
        let owners = actions
            .iter()
            .filter_map(|v| v["owner"].as_u64())
            .collect::<BTreeSet<_>>();
        if receipt != audit.receipt_contract
            || expected.len() != 857
            || actions.len() != 857
            || owners.len() != 18
            || actions.iter().filter(|v| v["op"] == "update").count() != 829
            || actions.iter().filter(|v| v["op"] == "credit").count() != 28
            || report(&expected, owners.len(), Differences::default()) != receipt
        {
            return Err("gap original857/18/187/9 receipt contract differs".into());
        }
        Ok(Self {
            root: root.into(),
            root_before: fs::symlink_metadata(root)?,
            archive_root: archive_root.into(),
            archive_root_before: fs::symlink_metadata(archive_root)?,
            input,
            expected_bytes,
            expected,
            audit,
            held,
        })
    }

    fn verify(&self) -> Result<()> {
        stage_roster(&self.root)?;
        owned_directory(&self.archive_root)?;
        if !identity(&self.root_before, &fs::symlink_metadata(&self.root)?)
            || !identity(
                &self.archive_root_before,
                &fs::symlink_metadata(&self.archive_root)?,
            )
        {
            return Err("gap stage/archive directory was replaced or changed".into());
        }
        for file in &self.held {
            file.verify()?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Differences {
    count: usize,
    first: Vec<Value>,
}

impl Differences {
    fn push(&mut self, value: Value) {
        self.count += 1;
        if self.first.len() < 50 {
            self.first.push(value);
        }
    }
}

fn compare(expected: &Value, actual: &Value, path: &str, rows: &mut Differences) {
    match (expected, actual) {
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() || a.keys().any(|key| !b.contains_key(key)) {
                rows.push(json!({"path":path,"expected_keys":a.keys().collect::<Vec<_>>(),"actual_keys":b.keys().collect::<Vec<_>>() }));
                return;
            }
            for (key, value) in a {
                compare(value, &b[key], &format!("{path}.{key}"), rows);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                rows.push(json!({"path":path,"expected_length":a.len(),"actual_length":b.len()}));
                return;
            }
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{index}]"), rows);
            }
        }
        (Value::Number(a), _) if a.is_f64() => {
            if actual.as_f64().map(f64::to_bits) != a.as_f64().map(f64::to_bits) {
                rows.push(json!({"path":path,"expected":expected,"actual":actual}));
            }
        }
        _ if expected != actual => {
            rows.push(json!({"path":path,"expected":expected,"actual":actual}))
        }
        _ => {}
    }
}

fn report(rows: &[Value], owners: usize, differences: Differences) -> Receipt {
    Receipt {
        actions: rows.len(),
        owners,
        positive_plans: rows
            .iter()
            .filter(|v| v["plan"]["confidence"].as_f64().is_some_and(|n| n > 0.))
            .count(),
        positive_credit: rows
            .iter()
            .filter(|v| {
                v.as_array()
                    .is_some_and(|a| a.iter().any(|n| n.as_u64().is_some_and(|n| n != 0)))
            })
            .count(),
        mismatches: differences.count,
        first_mismatches: differences.first,
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}

fn group_guard(pid: i32, group: i32) -> Result<()> {
    if pid != group {
        return Err("gap requires the outer NativeRunner owned group leader".into());
    }
    Ok(())
}

fn current_job(options: &Options) -> Result<(PathBuf, HeldFile, HeldFile)> {
    group_guard(
        getpid().as_raw_nonzero().get(),
        getpgrp().as_raw_nonzero().get(),
    )?;
    let job = PathBuf::from(std::env::var("CI_JOB_TEMP")?);
    let source = std::env::var("GITHUB_SHA")?;
    let expected_seconds = match std::env::consts::ARCH {
        "x86_64" => 2700,
        "aarch64" => 5400,
        _ => return Err("unsupported gap host".into()),
    };
    if std::env::consts::OS != "linux"
        || std::env::var("GITHUB_ACTIONS")? != "true"
        || std::env::var("GITHUB_REPOSITORY")? != "bin9208/openpilot-rust"
        || std::env::var("GITHUB_JOB")? != "planner-runtime"
        || source.len() != 40
        || !source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || std::env::var("NATIVE_TOOLS_HOST")?
            != format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
        || options.root.as_path() != Path::new(&std::env::var("CI296_SOURCE")?)
        || options.root.canonicalize()? != options.root
        || job.parent() != Some(Path::new("/tmp"))
        || !job.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
            v.strip_prefix("openpilot-ci.")
                .is_some_and(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_alphanumeric()))
        })
        || options.corpus != job.join("planner-gap-original")
        || options.audit != options.corpus.join("audit.json")
        || options.output != job.join("planner-comparison/gap")
        || options.binary
            != options
                .root
                .join("rust/target/debug/examples/plannerd_gap_trace")
        || options.timeout != Duration::from_secs(expected_seconds)
        || options.growth_bytes != 128 * MIB
    {
        return Err("gap requires the fixed native runtime source/host/paths/budgets".into());
    }
    owned_directory(&job)?;
    owned_directory(&job.join("planner-comparison"))?;
    let owner = format!(
        "{}:{}:{}",
        std::env::var("GITHUB_RUN_ID")?,
        std::env::var("GITHUB_RUN_ATTEMPT")?,
        std::env::var("GITHUB_JOB")?
    );
    let (_, owner) = HeldFile::open(
        &job.join("owner"),
        owner.len() as u64,
        &hash(owner.as_bytes()),
        false,
    )?;
    let metadata = fs::symlink_metadata(&options.binary)?;
    if !metadata.is_file()
        || metadata.len() > 256 * MIB
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err("gap native trace executable required".into());
    }
    let digest = crate::seal::digest(&options.binary)?;
    let (_, binary) = HeldFile::open(&options.binary, metadata.len(), &digest, false)?;
    Ok((job, owner, binary))
}

fn command(options: &Options, input: &[u8], deadline: Instant) -> Result<CommandSpec> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("gap operation deadline")?;
    if remaining.is_zero() {
        return Err("gap operation deadline".into());
    }
    let mut spec = CommandSpec::new(
        vec![options
            .binary
            .to_str()
            .ok_or("gap trace UTF-8 path")?
            .into()],
        remaining,
    );
    spec.cwd = Some(options.root.clone());
    spec.input = Some(input.to_vec());
    spec.stdout_limit = 8 * MIB as usize;
    spec.stderr_limit = MIB as usize;
    Ok(spec)
}

fn capture_trace(
    spec: &CommandSpec,
    output: &Path,
    run: impl FnOnce(&CommandSpec) -> Result<Captured>,
) -> Result<Captured> {
    write_json(
        &output.join("invocation.json"),
        &json!({"argv":spec.effective_argv().iter().map(|v|v.to_string_lossy()).collect::<Vec<_>>(),
            "timeout_milliseconds":spec.timeout.as_millis(),"stdout_limit":spec.stdout_limit,"stderr_limit":spec.stderr_limit}),
    )?;
    let captured = run(spec);
    let (stdout, stderr) = match &captured {
        Ok(result) => (result.stdout.as_slice(), result.stderr.as_slice()),
        Err(error) => match error.downcast_ref::<CaptureError>() {
            Some(CaptureError::Timeout { stdout, stderr, .. }) => {
                (stdout.as_slice(), stderr.as_slice())
            }
            _ => (&[][..], &[][..]),
        },
    };
    write_new(&output.join("actual.json"), stdout)?;
    write_new(&output.join("stderr.txt"), stderr)?;
    write_json(
        &output.join("trace-command.json"),
        &json!({"argv":spec.effective_argv().iter().map(|v|v.to_string_lossy()).collect::<Vec<_>>(),"returncode":captured.as_ref().ok().map(|r|r.code),"error":captured.as_ref().err().map(ToString::to_string)}),
    )?;
    let result = captured?;
    if result.code != 0 {
        // 원래 Python check_returncode의 미처리 예외는 leaf exit1이었다. raw trace code는 위에 보존한다.
        return Err(format!(
            "gap trace exited {}; original wrapper leaf exits 1",
            result.code
        )
        .into());
    }
    Ok(result)
}

pub(crate) fn run(options: &Options) -> Result<Receipt> {
    let deadline = Instant::now()
        .checked_add(options.timeout)
        .ok_or("gap deadline overflow")?;
    let (job, owner, binary) = current_job(options)?;
    let corpus = Corpus::open(
        &options.corpus,
        &options.audit,
        &job.join("planner-lifecycle-original"),
        std::env::consts::ARCH,
    )?;
    openpilot_disk_space::inspect(
        options.output.parent().ok_or("gap output parent")?,
        options.growth_bytes,
    )?
    .require()?;
    fs::DirBuilder::new().mode(0o700).create(&options.output)?;
    write_new(&options.output.join("input.json"), &corpus.input)?;
    write_new(
        &options.output.join("expected.json"),
        &corpus.expected_bytes,
    )?;
    let work = (|| -> Result<Receipt> {
        let spec = command(options, &corpus.input, deadline)?;
        let captured = capture_trace(&spec, &options.output, |spec| {
            NativeRunner.run_in_current_group(spec)
        })?;
        let actual: Vec<Value> = serde_json::from_slice(&captured.stdout)?;
        let mut differences = Differences::default();
        compare(
            &Value::Array(corpus.expected.clone()),
            &Value::Array(actual.clone()),
            "",
            &mut differences,
        );
        let receipt = report(&actual, 18, differences);
        write_json(&options.output.join("receipt.json"), &receipt)?;
        if receipt != corpus.audit.receipt_contract {
            return Err("gap original/native857 mismatch; see receipt.json".into());
        }
        Ok(receipt)
    })();
    // 오류 경로도 같은 corpus/archive/실행 파일의 교체 여부를 반환 전에 확인한다.
    let sealed = corpus
        .verify()
        .and_then(|()| owner.verify())
        .and_then(|()| binary.verify());
    write_json(
        &options.output.join("native-gap-provenance.json"),
        &json!({
            "source_commit":std::env::var("GITHUB_SHA")?,"original_source_commit":SOURCE,
            "input_sha256":corpus.audit.input.sha256,"expected_sha256":corpus.audit.expected.sha256,
            "original_artifact":corpus.audit.artifact,"audit_sha256":hash(qualified_audit(std::env::consts::ARCH)?),
            "binary_sha256":binary.sha256,"postflight_error":sealed.as_ref().err().map(ToString::to_string),
            "scope":"frozen original841 gap857 equality; no Python invocation or full-runtime PASS"
        }),
    )?;
    let receipt = work?;
    sealed?;
    if Instant::now() >= deadline {
        return Err("gap operation deadline".into());
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "gap_tests.rs"]
mod gap_tests;
