//! 고정 artifact의 SSH39 원문만 수용한다. 원본 실행/expected 생성/삭제 수용은 없다.
use super::{Case, Value};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Cursor, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Component, Path, PathBuf},
};

type Admission<T> = Result<T, Box<dyn std::error::Error>>;
const MARKER: &[u8] = b"DEVELOPER_CLI_ORIGINAL_JSON ";
const READ: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
    .union(rustix::fs::OFlags::NOFOLLOW)
    .union(rustix::fs::OFlags::NONBLOCK)
    .union(rustix::fs::OFlags::CLOEXEC);

struct Spec {
    group: &'static str,
    count: usize,
    artifact: u64,
    run: u64,
    source: &'static str,
    packed: usize,
    archive_sha: &'static str,
    entries: usize,
    expanded: u64,
    log: &'static str,
    log_bytes: u64,
    log_crc: u32,
    log_sha: &'static str,
    raw_bytes: usize,
    raw_sha: &'static str,
    selector: &'static str,
}
const SPECS: [Spec; 2] = [
    Spec {
        group: "ssh-plan",
        count: 26,
        artifact: 12563952000,
        run: 40103403093,
        source: "5d376b306c93ec799b0c5346cb60e9425b1618f7",
        packed: 15_834_369,
        archive_sha: "f32e796ddd7e8e4b5ff9c50d0af1bddefe460a6d4b5b489d975e449483a4ef6c",
        entries: 55,
        expanded: 24_465_234,
        log: "ssh-alias-original.log",
        log_bytes: 42_741,
        log_crc: 0x868a933b,
        log_sha: "a1ac7a4514be5ec6ccf7e73f0ed23e6364f4e71ed6e8553d453feb29e7e3dbdb",
        raw_bytes: 21_957,
        raw_sha: "c7ee0ba37a74aa9c58bd9c333281b8405548c2267e832f6606b5ac0311bd4b46",
        selector: "ssh_cli_original_aliases_prefix_unicode_duplicate_order_and_exec",
    },
    Spec {
        group: "ssh-parser",
        count: 13,
        artifact: 12560030996,
        run: 40092771379,
        source: "fb1228f02e0546b6fe7a9d8d0eec098e650a86bd",
        packed: 7_775_163,
        archive_sha: "21c33f18576748c77664d0267b4c51edca8aedf0f8b3c345e8e91d477d739878",
        entries: 47,
        expanded: 14_349_404,
        log: "ssh-parser-original.log",
        log_bytes: 11_643,
        log_crc: 0xc6015ad0,
        log_sha: "3c6ac45ece10931d5f0149e3bba3d0f99d0afdf55053491ce580055183216a29",
        raw_bytes: 9_313,
        raw_sha: "8ce2735003cd58258142aa16c9716988da143cff37b2d8ce1a920df58ae27076",
        selector: "ssh_cli_original_port_argparse_help_and_no_auth_effects",
    },
];
fn require(ok: bool, error: &str) -> Admission<()> {
    if ok {
        Ok(())
    } else {
        Err(error.into())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn spec(group: &str) -> Admission<&'static Spec> {
    SPECS
        .iter()
        .find(|s| s.group == group)
        .ok_or_else(|| "SSH frozen scope differs".into())
}
fn keys(value: &Value, names: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == names.len() && names.iter().all(|name| v.contains_key(*name)))
}
// atime는 읽기 자체로 변한다. 나머지 data/metadata identity는 전후 동일해야 한다.
#[derive(Debug, Eq, PartialEq)]
struct Identity(u64, u64, u32, u32, u32, u64, u64, i64, i64, i64, i64);
fn identity(m: &fs::Metadata) -> Identity {
    Identity(
        m.dev(),
        m.ino(),
        m.mode(),
        m.uid(),
        m.gid(),
        m.nlink(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn directory(path: &Path) -> Admission<File> {
    require(path.is_absolute(), "SSH context must be absolute")?;
    let mut file = File::from(rustix::fs::open(
        "/",
        READ.union(rustix::fs::OFlags::DIRECTORY),
        rustix::fs::Mode::empty(),
    )?);
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                file = File::from(rustix::fs::openat(
                    &file,
                    Path::new(name),
                    READ.union(rustix::fs::OFlags::DIRECTORY),
                    rustix::fs::Mode::empty(),
                )?)
            }
            _ => return Err("SSH context path is not normal".into()),
        }
    }
    let m = file.metadata()?;
    require(
        fs::canonicalize(path)?.as_os_str() == path.as_os_str(),
        "SSH context must be canonical",
    )?;
    require(
        m.is_dir() && m.uid() == fs::metadata("/proc/self")?.uid() && m.mode() & 0o777 == 0o700,
        "SSH context must be owned private0700",
    )?;
    Ok(file)
}
struct Held {
    path: PathBuf,
    file: File,
    before: Identity,
    sha: String,
    bytes: Vec<u8>,
}
impl Held {
    fn open(parent: &File, path: PathBuf, max: usize) -> Admission<Self> {
        let name = path.file_name().ok_or("SSH input name missing")?;
        let file = File::from(rustix::fs::openat(
            parent,
            Path::new(name),
            READ,
            rustix::fs::Mode::empty(),
        )?);
        let m = file.metadata()?;
        require(
            m.is_file()
                && m.uid() == parent.metadata()?.uid()
                && m.nlink() == 1
                && m.mode() & 0o077 == 0
                && m.len() <= max as u64,
            "SSH input is not bounded private regular data",
        )?;
        let before = identity(&m);
        let mut bytes = vec![0; m.len() as usize];
        let mut offset = 0;
        while offset < bytes.len() {
            let end = bytes.len().min(offset + 65_536);
            let n = file.read_at(&mut bytes[offset..end], offset as u64)?;
            require(n > 0, "SSH input short read")?;
            offset += n;
        }
        let mut extra = [0; 1];
        require(file.read_at(&mut extra, m.len())? == 0, "SSH input grew")?;
        let held = Self {
            path,
            file,
            before,
            sha: digest(&bytes),
            bytes,
        };
        held.verify()?;
        Ok(held)
    }
    fn verify(&self) -> Admission<()> {
        require(
            identity(&self.file.metadata()?) == self.before
                && identity(&fs::symlink_metadata(&self.path)?) == self.before,
            "SSH held input identity changed",
        )?;
        let mut hash = Sha256::new();
        let mut offset = 0_u64;
        let mut buffer = [0; 65_536];
        loop {
            let n = self.file.read_at(&mut buffer, offset)?;
            if n == 0 {
                break;
            }
            offset += n as u64;
            require(offset <= self.before.6, "SSH held input grew")?;
            hash.update(&buffer[..n]);
        }
        require(
            offset == self.before.6
                && format!("{:x}", hash.finalize()) == self.sha
                && identity(&self.file.metadata()?) == self.before
                && identity(&fs::symlink_metadata(&self.path)?) == self.before,
            "SSH held input bytes changed",
        )
    }
}
struct Context {
    path: PathBuf,
    dir: File,
    before: Identity,
    files: Vec<Held>,
}
fn names(dir: &File) -> Admission<BTreeSet<String>> {
    fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))?
        .map(|entry| {
            Ok(entry?
                .file_name()
                .into_string()
                .map_err(|_| "SSH context name is not UTF8")?)
        })
        .collect()
}
fn expected_names() -> BTreeSet<String> {
    SPECS
        .iter()
        .flat_map(|s| ["zip", "json"].map(|ext| format!("artifact-{}.{ext}", s.artifact)))
        .collect()
}
impl Context {
    fn open() -> Admission<Self> {
        let path = PathBuf::from(
            std::env::var_os("DEVELOPER_SSH_FROZEN_DIR")
                .ok_or("DEVELOPER_SSH_FROZEN_DIR required")?,
        );
        let dir = directory(&path)?;
        require(
            names(&dir)? == expected_names(),
            "SSH context roster differs",
        )?;
        let before = identity(&dir.metadata()?);
        let mut files = Vec::new();
        for s in &SPECS {
            files.push(Held::open(
                &dir,
                path.join(format!("artifact-{}.zip", s.artifact)),
                s.packed,
            )?);
            files.push(Held::open(
                &dir,
                path.join(format!("artifact-{}.json", s.artifact)),
                16_384,
            )?);
        }
        for (s, pair) in SPECS.iter().zip(files.chunks_exact(2)) {
            require(
                pair[0].bytes.len() == s.packed && pair[0].sha == s.archive_sha,
                "SSH context archive pin differs",
            )?;
            validate_artifact(&serde_json::from_slice(&pair[1].bytes)?, s)?;
        }
        let context = Self {
            path,
            dir,
            before,
            files,
        };
        context.verify()?;
        Ok(context)
    }
    fn verify(&self) -> Admission<()> {
        require(
            identity(&self.dir.metadata()?) == self.before
                && identity(&directory(&self.path)?.metadata()?) == self.before
                && names(&self.dir)? == expected_names(),
            "SSH context changed",
        )?;
        for held in &self.files {
            held.verify()?;
        }
        Ok(())
    }
    fn import(&self, s: &Spec) -> Admission<Vec<u8>> {
        let index = SPECS
            .iter()
            .position(|candidate| candidate.group == s.group)
            .ok_or("SSH group missing")?
            * 2;
        let metadata: Value = serde_json::from_slice(&self.files[index + 1].bytes)?;
        validate_artifact(&metadata, s)?;
        archive(&self.files[index].bytes, s.archive_sha, s)
    }
}
fn validate_artifact(v: &Value, s: &Spec) -> Admission<()> {
    require(
        v["id"] == s.artifact
            && v["size_in_bytes"] == s.packed as u64
            && v["name"] == format!("diagnostic-fast-focused-{}-1", s.run)
            && v["digest"] == format!("sha256:{}", s.archive_sha)
            && v["expired"] == false
            && v["workflow_run"]["id"] == s.run
            && v["workflow_run"]["head_sha"] == s.source
            && v["workflow_run"]["head_branch"] == "codex/feat-523-python-rust"
            && v["workflow_run"]["repository_id"] == 1395501963_u64
            && v["workflow_run"]["head_repository_id"] == 1395501963_u64,
        "SSH artifact metadata binding differs",
    )
}
fn archive(bytes: &[u8], pin: &str, s: &Spec) -> Admission<Vec<u8>> {
    require(pin == s.archive_sha, "SSH fixed archive pin differs")?;
    require(bytes.len() == s.packed, "SSH archive length differs")?;
    require(digest(bytes) == s.archive_sha, "SSH archive SHA differs")?;
    zip_payload(bytes, s)
}
// 정상 경로는 고정 archive SHA를 먼저 검증한다. 아래 분리는 CRC 음성 주입에만 쓴다.
fn zip_payload(bytes: &[u8], s: &Spec) -> Admission<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    require(zip.len() == s.entries, "SSH ZIP member count differs")?;
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    let mut log = None;
    let mut source = None;
    let mut buffer = [0; 65_536];
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = std::str::from_utf8(entry.name_raw())?.to_owned();
        require(
            !name.is_empty()
                && !name.contains(['\\', '\0'])
                && !name.starts_with('/')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
                && name
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
                && names.insert(name.clone())
                && entry.is_file()
                && !entry.encrypted()
                && entry.compression() == zip::CompressionMethod::Deflated
                && entry
                    .unix_mode()
                    .is_none_or(|mode| matches!(mode & 0o170000, 0 | 0o100000)),
            "SSH ZIP duplicate/unsafe member",
        )?;
        total = total
            .checked_add(entry.size())
            .ok_or("SSH ZIP size overflow")?;
        require(
            total <= s.expanded && entry.size() <= s.expanded,
            "SSH ZIP inflation bound",
        )?;
        if name == s.log {
            require(
                entry.size() == s.log_bytes && entry.crc32() == s.log_crc,
                "SSH selected log size/CRC differs",
            )?;
        }
        let keep = name == s.log || name == "SOURCE_COMMIT";
        let mut selected = Vec::new();
        let mut count = 0_u64;
        loop {
            let n = entry.read(&mut buffer)?; // EOF까지 읽어 모든 member CRC를 검사한다.
            if n == 0 {
                break;
            }
            count += n as u64;
            require(
                count <= entry.size() && count <= s.expanded,
                "SSH ZIP member exceeds bound",
            )?;
            if keep {
                require(count <= s.log_bytes.max(41), "SSH selected log grew")?;
                selected.extend_from_slice(&buffer[..n]);
            }
        }
        require(count == entry.size(), "SSH ZIP member truncated")?;
        if name == s.log {
            log = Some(selected);
        } else if name == "SOURCE_COMMIT" {
            source = Some(selected);
        }
    }
    require(
        total == s.expanded && source.as_deref() == Some(format!("{}\n", s.source).as_bytes()),
        "SSH ZIP source/expanded size differs",
    )?;
    let log = log.ok_or("SSH original log missing")?;
    require(
        log.len() == s.log_bytes as usize && digest(&log) == s.log_sha && log.ends_with(b"\n"),
        "SSH original log pin differs",
    )?;
    let text = std::str::from_utf8(&log)?;
    require(
        text.contains(s.selector)
            && text
                .lines()
                .filter(|line| {
                    line.starts_with(
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in ")
                })
                .count()
                == 1,
        "SSH original test did not pass",
    )?;
    let payloads = log
        .split(|b| *b == b'\n')
        .filter_map(|line| {
            line.windows(MARKER.len())
                .position(|part| part == MARKER)
                .map(|offset| &line[offset + MARKER.len()..])
        })
        .collect::<Vec<_>>();
    require(payloads.len() == 1, "SSH original marker count differs")?;
    Ok(payloads[0].to_vec())
}
fn historical_metadata(s: &Spec) -> Value {
    let mut metadata = json!({
        "python":"3.12.15 (main, Oct  1 2026, 02:35:59) [GCC 13.3.0]", "requests":"2.34.2",
        "sources":{
            "tools/scripts/setup_ssh_keys.py":"ab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f",
            "tools/scripts/ssh.py":"f4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff"
        }
    });
    if s.group == "ssh-plan" {
        metadata["native_sources"] = json!({
            "rust/crates/dev-tools/src/developer_ssh.rs":"45f5c9880964d2ce954f02997cf0b85c8b429051aa6900daaa822258f07546ee",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.LICENSE-UNICODE-V3.txt":"e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.provenance.json":"5ad5796e126319c60785f07707e22902de2e935d1343164d0898bd1f6325b678",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15.rs":"f0edde8b8443486b6940dc8f22e7cea65675b8230a46baec11c8ce0c2bab6b40",
            "rust/crates/dev-tools/src/developer_ssh/unicode_lower15_original.rs":"bf2758c92e1cfdc9f69249ce1c16b6a96e393ad062adf259e15d6120148e38a0"
        });
    }
    metadata
}
fn validate(s: &Spec, v: &Value, cases: &[Case]) -> Admission<()> {
    require(
        keys(v, &["schema", "group", "metadata", "records"]),
        "SSH envelope object differs",
    )?;
    require(
        v["schema"] == "openpilot.developer-cli-original.v1" && v["group"] == s.group,
        "SSH envelope scope differs",
    )?;
    require(
        v["metadata"] == historical_metadata(s),
        "SSH historical metadata differs",
    )?;
    let records = v["records"]
        .as_array()
        .ok_or("SSH records array required")?;
    require(records.len() == s.count, "SSH record count differs")?;
    let mut ids = BTreeSet::new();
    for record in records {
        require(
            keys(record, &["input", "criteria", "expected_original"]),
            "SSH record object differs",
        )?;
        let input = &record["input"];
        require(input["tool"] == "ssh", "SSH record tool differs")?;
        let id = input["id"].as_str().ok_or("SSH string id required")?;
        require(!id.is_empty() && ids.insert(id), "SSH ids must be unique")?;
        let output = &record["expected_original"];
        require(
            keys(
                output,
                &[
                    "stdout",
                    "stderr",
                    "exit_code",
                    "exception",
                    "effects",
                    "writes",
                    "exec",
                ],
            ) && output["stdout"].is_string()
                && output["stderr"].is_string()
                && (output["exit_code"].is_null() || output["exit_code"].is_i64())
                && output["effects"].is_array()
                && output["writes"].is_array(),
            "SSH original output type differs",
        )?;
        let exception = &output["exception"];
        require(
            exception.is_null()
                || (keys(exception, &["type", "message"])
                    && exception["type"].is_string()
                    && exception["message"].is_string()),
            "SSH original exception type differs",
        )?;
        let exec = &output["exec"];
        require(
            exec.is_null()
                || (keys(exec, &["file", "argv"])
                    && exec["file"] == "ssh"
                    && exec["argv"]
                        .as_array()
                        .is_some_and(|args| args.iter().all(Value::is_string))),
            "SSH original exec type differs",
        )?;
    }
    require(cases.len() == s.count, "SSH authored case count differs")?;
    for (case, record) in cases.iter().zip(records) {
        require(case.input == record["input"], "SSH authored input differs")?;
        require(
            case.criteria == record["criteria"],
            "SSH authored criteria differs",
        )?;
    }
    Ok(())
}
fn admit_bytes(raw: &[u8], pin: &str, s: &Spec, cases: &[Case]) -> Admission<Value> {
    require(pin == s.raw_sha, "SSH fixed raw pin differs")?;
    require(raw.len() == s.raw_bytes, "SSH raw length differs")?;
    require(digest(raw) == s.raw_sha, "SSH raw SHA differs")?;
    let envelope: Value = serde_json::from_slice(raw)?;
    validate(s, &envelope, cases)?;
    Ok(envelope)
}
pub(super) fn admit(group: &str, cases: &[Case]) -> Admission<(Vec<u8>, Value)> {
    let s = spec(group)?;
    let context = Context::open()?;
    let raw = context.import(s)?;
    let envelope = admit_bytes(&raw, s.raw_sha, s, cases)?;
    context.verify()?;
    println!(
        "DEVELOPER_SSH_FROZEN_ADMISSION {}",
        json!({
            "schema":1,"group":group,"artifact":s.artifact,"run":s.run,"source":s.source,
            "archive_sha256":s.archive_sha,"crc_checked_members":s.entries,"raw_sha256":s.raw_sha,
            "records":s.count,"current_provider_closure_complete":false,"historical_provider_closure_complete":false,
            "actual_ssh_cli_executed":false,"retirement_credit":0
        })
    );
    Ok((raw, envelope))
}

// fixture-derived cases는 음성 통제의 baseline 전용이다. 정상39건은 부모의 authored body가 만든다.
fn rejection_inputs(s: &Spec) -> (Context, Vec<u8>, Value, Vec<Case>) {
    let context = Context::open().unwrap();
    let raw = context.import(s).unwrap();
    let envelope: Value = serde_json::from_slice(&raw).unwrap();
    let cases = envelope["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| Case {
            input: record["input"].clone(),
            criteria: record["criteria"].clone(),
        })
        .collect::<Vec<_>>();
    admit_bytes(&raw, s.raw_sha, s, &cases).unwrap();
    (context, raw, envelope, cases)
}
#[test]
fn ssh_frozen_rejects_archive_and_raw_pin_tampering() {
    for s in &SPECS {
        let (context, raw, _, cases) = rejection_inputs(s);
        let index = SPECS
            .iter()
            .position(|candidate| candidate.group == s.group)
            .unwrap()
            * 2;
        let packed = &context.files[index].bytes;
        assert!(archive(packed, &"0".repeat(64), s)
            .unwrap_err()
            .to_string()
            .contains("fixed archive pin"));
        let mut changed = packed.to_vec();
        changed[0] ^= 1;
        assert!(archive(&changed, s.archive_sha, s)
            .unwrap_err()
            .to_string()
            .contains("archive SHA"));
        assert!(archive(&packed[..packed.len() - 1], s.archive_sha, s)
            .unwrap_err()
            .to_string()
            .contains("archive length"));
        // 압축 bytes는 유지하고 첫 central entry CRC만 변조한다. 새 pin은 수용하지 않는다.
        let eocd = packed.len() - 22;
        assert_eq!(&packed[eocd..eocd + 4], b"PK\x05\x06");
        let central = u32::from_le_bytes(packed[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        assert_eq!(&packed[central..central + 4], b"PK\x01\x02");
        let mut bad_crc = packed.to_vec();
        bad_crc[central + 16] ^= 1;
        assert!(
            zip_payload(&bad_crc, s).is_err(),
            "central CRC change accepted"
        );
        assert!(admit_bytes(&raw, &"0".repeat(64), s, &cases)
            .unwrap_err()
            .to_string()
            .contains("fixed raw pin"));
        let mut changed = raw.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(admit_bytes(&changed, s.raw_sha, s, &cases)
            .unwrap_err()
            .to_string()
            .contains("raw SHA"));
        assert!(admit_bytes(&raw[..raw.len() - 1], s.raw_sha, s, &cases)
            .unwrap_err()
            .to_string()
            .contains("raw length"));
        let mut extra = raw;
        extra.push(b'\n');
        assert!(admit_bytes(&extra, s.raw_sha, s, &cases)
            .unwrap_err()
            .to_string()
            .contains("raw length"));
        context.verify().unwrap();
    }
}
#[test]
fn ssh_frozen_rejects_metadata_scope_missing_duplicate_and_types() {
    assert!(spec("setup-effects").is_err());
    for s in &SPECS {
        let (context, _, envelope, cases) = rejection_inputs(s);
        let index = SPECS
            .iter()
            .position(|candidate| candidate.group == s.group)
            .unwrap()
            * 2;
        let metadata: Value = serde_json::from_slice(&context.files[index + 1].bytes).unwrap();
        validate_artifact(&metadata, s).unwrap();
        for pointer in [
            "/id",
            "/digest",
            "/workflow_run/id",
            "/workflow_run/head_sha",
            "/workflow_run/repository_id",
        ] {
            let mut bad = metadata.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::Null;
            assert!(validate_artifact(&bad, s).is_err(), "{pointer}");
        }
        for (pointer, value) in [
            ("/group", json!("setup-effects")),
            ("/metadata/requests", json!("current")),
            ("/records/0/input/tool", json!("setup")),
            ("/records/0/input/id", json!(1)),
            ("/records/0/expected_original/stdout", json!([])),
        ] {
            let mut bad = envelope.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(validate(s, &bad, &cases).is_err(), "{pointer}");
        }
        let mut bad = envelope.clone();
        bad["metadata"]["current_provider"] = json!("not historical");
        assert!(validate(s, &bad, &cases).is_err());
        let mut bad = envelope.clone();
        bad["records"].as_array_mut().unwrap().pop();
        assert!(validate(s, &bad, &cases).is_err());
        let mut bad = envelope.clone();
        let first = bad["records"][0].clone();
        bad["records"].as_array_mut().unwrap().push(first);
        assert!(validate(s, &bad, &cases).is_err());
        let mut bad = envelope;
        bad["records"][1]["input"]["id"] = bad["records"][0]["input"]["id"].clone();
        assert!(validate(s, &bad, &cases)
            .unwrap_err()
            .to_string()
            .contains("ids must be unique"));
        context.verify().unwrap();
    }
}
#[test]
fn ssh_frozen_rejects_authored_input_criteria_and_order_drift() {
    for s in &SPECS {
        let (context, _, envelope, mut cases) = rejection_inputs(s);
        cases[0].input["args"] = json!(["changed"]);
        assert!(validate(s, &envelope, &cases)
            .unwrap_err()
            .to_string()
            .contains("authored input"));
        cases[0].input = envelope["records"][0]["input"].clone();
        cases[0].criteria = json!({"kind":"exact","extra":true});
        assert!(validate(s, &envelope, &cases)
            .unwrap_err()
            .to_string()
            .contains("authored criteria"));
        cases[0].criteria = envelope["records"][0]["criteria"].clone();
        cases.swap(0, 1);
        assert!(validate(s, &envelope, &cases)
            .unwrap_err()
            .to_string()
            .contains("authored input"));
        context.verify().unwrap();
    }
}
#[test]
fn ssh_frozen_held_inputs_reject_mutation_replacement_and_aliases() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for mode in ["same-size", "truncate", "replace", "symlink", "hardlink"] {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = temp.path().join("data");
        fs::write(&path, b"private-inert-data").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let parent = directory(temp.path()).unwrap();
        let mut held = Held::open(&parent, path.clone(), 1024).unwrap();
        match mode {
            "same-size" => {
                fs::write(&path, b"changed-inert-data").unwrap();
                // test 내부 metadata를 재바인딩해도 이전 SHA가 독립적으로 거부해야 한다.
                held.before = identity(&held.file.metadata().unwrap());
            }
            "truncate" => fs::write(&path, b"short").unwrap(),
            "replace" => {
                fs::remove_file(&path).unwrap();
                fs::write(&path, b"private-inert-data").unwrap();
            }
            "symlink" => {
                let other = temp.path().join("other");
                fs::write(&other, b"private-inert-data").unwrap();
                fs::remove_file(&path).unwrap();
                symlink(other, &path).unwrap();
            }
            "hardlink" => fs::hard_link(&path, temp.path().join("alias")).unwrap(),
            _ => unreachable!(),
        }
        assert!(held.verify().is_err(), "{mode}");
    }
}
