//! #482 Actions 전용 D1. 기존 strict 실패는 관측 후에도 성공으로 승격하지 않는다.
use super::*;
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::driving_daemon_check::{self, source_policy::SourcePolicy, Options};
use std::{
    io::{Cursor, Write},
    os::unix::fs::OpenOptionsExt,
    time::Instant,
};

const FIXTURE: &str = "afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d";
const RAW: &str = "9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe";

// #584: private 단계 관측 시작. 기존 runtime 내부를 계측하는 hook은 아니다.
const MAX_STAGE_FILES: usize = 32;
const MAX_STAGE_BYTES: usize = 4096;
const MAX_STAGE_TOTAL: usize = 128 * 1024;
const D1_STAGES: [&str; 28] = [
    "fixture-open-before",
    "fixture-open-after",
    "current-open-before",
    "current-open-after",
    "original-open-before",
    "original-open-after",
    "frozen-daemon-admission-before",
    "frozen-daemon-admission-after",
    "run-only-dual-before",
    "run-only-dual-return",
    "packet-open-before",
    "packet-open-after",
    "packet-decode-before",
    "packet-decode-after",
    "whole-raw-diff-before",
    "whole-raw-diff-after",
    "input-guards-before",
    "input-guards-after",
    "fixture-verify-before",
    "fixture-verify-after",
    "original-verify-before",
    "original-verify-after",
    "current-verify-before",
    "current-verify-after",
    "daemon-verify-before",
    "daemon-verify-after",
    "receipt-write-before",
    "receipt-write-after",
];

struct StageDirectory {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u32, u32, u32),
}
impl StageDirectory {
    fn identity(meta: &fs::Metadata) -> (u64, u64, u32, u32, u32) {
        (meta.dev(), meta.ino(), meta.mode(), meta.uid(), meta.gid())
    }
    fn open(path: PathBuf, file: File) -> Self {
        let meta = file.metadata().unwrap();
        assert!(meta.is_dir() && meta.mode() & 0o022 == 0);
        assert_eq!(meta.uid(), rustix::process::geteuid().as_raw());
        let value = Self {
            path,
            file,
            identity: Self::identity(&meta),
        };
        value.verify();
        value
    }
    fn verify(&self) {
        assert_eq!(self.path.canonicalize().unwrap(), self.path);
        let named = fs::symlink_metadata(&self.path).unwrap();
        assert!(named.is_dir() && !named.file_type().is_symlink());
        assert_eq!(Self::identity(&named), self.identity);
        assert_eq!(
            Self::identity(&self.file.metadata().unwrap()),
            self.identity
        );
    }
}

struct D1Stages {
    directories: Vec<StageDirectory>,
    records: Vec<DataSeal>,
    started: Instant,
    elapsed_ns: u64,
    bytes: usize,
    source: String,
    test_sha256: String,
}
impl D1Stages {
    fn new(job: &Path, source: &str) -> Self {
        assert!(source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(env::var("EXPECTED_SOURCE").unwrap(), source);
        let started = Instant::now();
        let flags = rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC;
        let fd = rustix::fs::open(job, flags, rustix::fs::Mode::empty()).unwrap();
        let mut directories = vec![StageDirectory::open(job.into(), File::from(fd))];
        assert_eq!(directories[0].identity.2 & 0o7777, 0o700);
        for name in ["proof", "model482", "d1-stages"] {
            let parent = directories.last().unwrap();
            parent.verify();
            if name == "d1-stages" {
                rustix::fs::mkdirat(&parent.file, name, rustix::fs::Mode::from_raw_mode(0o700))
                    .unwrap();
            }
            let fd =
                rustix::fs::openat(&parent.file, name, flags, rustix::fs::Mode::empty()).unwrap();
            let directory = StageDirectory::open(parent.path.join(name), File::from(fd));
            if name == "d1-stages" {
                assert_eq!(directory.identity.2 & 0o7777, 0o700);
            }
            directories.push(directory);
        }
        Self {
            directories,
            records: Vec::new(),
            started,
            elapsed_ns: 0,
            bytes: 0,
            source: source.into(),
            test_sha256: format!(
                "{:x}",
                Sha256::digest(include_bytes!("model_482_pr1344.rs"))
            ),
        }
    }
    fn mark(&mut self, stage: &str) {
        self.record(stage, None);
    }
    fn record(&mut self, stage: &str, error: Option<&str>) {
        let index = self.records.len();
        assert!(index < MAX_STAGE_FILES);
        assert_eq!(D1_STAGES.get(index).copied(), Some(stage));
        for directory in &self.directories {
            directory.verify();
        }
        let elapsed_ns = u64::try_from(self.started.elapsed().as_nanos()).unwrap();
        assert!(elapsed_ns >= self.elapsed_ns);
        let outcome = (stage == "run-only-dual-return").then(|| match error {
            None => json!({"kind":"ok"}),
            Some(error) => {
                let mut end = error.len().min(1024);
                while !error.is_char_boundary(end) {
                    end -= 1;
                }
                json!({"kind":"err","error_prefix":&error[..end],
                    "error_bytes":error.len(),"error_truncated":end != error.len()})
            }
        });
        let bytes = serde_json::to_vec(&json!({"schema":1,"sequence":index+1,
            "stage":stage,"elapsed_ns":elapsed_ns,"source":self.source,
            "test_source_sha256":self.test_sha256,"run_result":outcome}))
        .unwrap();
        assert!(bytes.len() <= MAX_STAGE_BYTES);
        let total = self.bytes.checked_add(bytes.len()).unwrap();
        assert!(total <= MAX_STAGE_TOTAL);
        let directory = self.directories.last().unwrap();
        let name = format!("{:02}-{stage}.json", index + 1);
        // CREATE|EXCL는 create_new와 같은 기존파일 거부 경계다.
        let fd = rustix::fs::openat(
            &directory.file,
            name.as_str(),
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::from_raw_mode(0o600),
        )
        .unwrap();
        let mut file = File::from(fd);
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        let meta = file.metadata().unwrap();
        assert!(meta.is_file() && meta.nlink() == 1 && meta.mode() & 0o7777 == 0o600);
        assert_eq!(meta.uid(), rustix::process::geteuid().as_raw());
        let seal = DataSeal::open(&directory.path.join(name), MAX_STAGE_BYTES as u64);
        assert_eq!((seal.identity.0, seal.identity.1), (meta.dev(), meta.ino()));
        assert_eq!(seal.pin["bytes"], bytes.len());
        assert_eq!(seal.pin["sha256"], format!("{:x}", Sha256::digest(&bytes)));
        self.records.push(seal);
        self.bytes = total;
        self.elapsed_ns = elapsed_ns;
        for directory in &self.directories {
            directory.verify();
        }
    }
    fn finish(&self) {
        assert_eq!(self.records.len(), D1_STAGES.len());
        let directory = self.directories.last().unwrap();
        let mut names = fs::read_dir(&directory.path)
            .unwrap()
            .take(MAX_STAGE_FILES + 1)
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        let expected = D1_STAGES
            .iter()
            .enumerate()
            .map(|(index, stage)| OsString::from(format!("{:02}-{stage}.json", index + 1)))
            .collect::<Vec<_>>();
        assert_eq!(names, expected);
        for record in &self.records {
            record.verify();
        }
        for directory in &self.directories {
            directory.verify();
        }
    }
}
// #584: private 단계 관측 끝.

fn raw_packet(bytes: &[u8]) -> (u32, Vec<u8>) {
    assert!(bytes.len() <= 4_000_000);
    let mut cursor = Cursor::new(bytes);
    let message = serialize::read_message(&mut cursor, ReaderOptions::new()).unwrap();
    assert_eq!(
        cursor.position() as usize,
        bytes.len(),
        "packet trailing bytes"
    );
    let root = message.get_root::<event::Reader<'_>>().unwrap();
    let model = match root.which().unwrap() {
        event::Which::ModelV2(value) => value.unwrap(),
        _ => panic!("D1 packet union is not ModelV2"),
    };
    let raw = model.get_raw_predictions().unwrap();
    assert_eq!(raw.len(), 2576 * 4, "whole raw prediction extent");
    (model.get_frame_id(), raw.to_vec())
}

#[test]
#[ignore = "Actions A 전체 인증 뒤 same-VM current dual; local 실행 금지"]
fn same_vm_current_dual_first_packet() {
    for (key, value) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_WORKFLOW_REF", "bin9208/openpilot-rust/.github/workflows/python-rust-523-model-pr1344-replay.yml@refs/heads/codex/feat-523-python-rust"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("GITHUB_EVENT_NAME", "push"),
        ("GITHUB_REF", "refs/heads/codex/feat-523-python-rust"),
        ("GITHUB_JOB", "model-pipelines"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
    ] {
        assert_eq!(env::var(key).unwrap(), value, "{key}");
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    assert_eq!(root, required("DIAG_SOURCE"));
    let job = required("CI_JOB_TEMP");
    assert_eq!(job.canonicalize().unwrap(), job);
    assert_eq!(
        fs::read_to_string(job.join("owner")).unwrap().trim(),
        format!(
            "{}:{}:model-pipelines",
            env::var("GITHUB_RUN_ID").unwrap(),
            env::var("GITHUB_RUN_ATTEMPT").unwrap()
        )
    );
    let request = DataSeal::open(&required("MODEL482_D1_REQUEST"), 4 * 1024 * 1024);
    assert_eq!(
        request.pin["sha256"],
        env::var("MODEL482_D1_REQUEST_SHA256").unwrap()
    );
    let value: Value = serde_json::from_slice(&fs::read(&request.path).unwrap()).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["source"], env::var("GITHUB_SHA").unwrap());
    assert_eq!(
        value["test_source_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("model_482_pr1344.rs"))
        )
    );
    let admission = DataSeal::open(&required("MODEL482_A_RECEIPT"), 4 * 1024 * 1024);
    let a: Value = serde_json::from_slice(&fs::read(&admission.path).unwrap()).unwrap();
    assert_eq!(a["source"], value["source"]);
    assert_eq!(a["strict_triplet_all"], true);
    assert_eq!(a["host_isa_all"], true);
    assert_eq!(value["readonly_sha256"], admission.pin["sha256"]);
    let mut stages = D1Stages::new(&job, value["source"].as_str().unwrap());
    stages.mark("fixture-open-before");
    let fixture = Fixture::open(&required("DRIVING_FIXTURE_DIRECTORY"), FIXTURE).unwrap();
    stages.mark("fixture-open-after");
    stages.mark("current-open-before");
    let current = CurrentCatalog::open(&root, &fixture);
    stages.mark("current-open-after");
    stages.mark("original-open-before");
    let original = Original::open(
        &fixture.original_directory(),
        fixture.original_manifest_sha256(),
    )
    .unwrap();
    stages.mark("original-open-after");
    stages.mark("frozen-daemon-admission-before");
    let frozen = original.read("1344-dual/outputs/raw-pred-0.bin").unwrap();
    assert_eq!(frozen.len(), 10304);
    assert_eq!(format!("{:x}", Sha256::digest(&frozen)), RAW);
    let daemon = required("DRIVING_DAEMON_NATIVE_BINARY");
    let held = Held::open(&daemon);
    assert_eq!(value["daemon_sha256"], sha(&daemon));
    let output = job.join("build/model482/d1");
    assert!(!output.exists() && !output.is_symlink());
    stages.mark("frozen-daemon-admission-after");
    stages.mark("run-only-dual-before");
    let result = driving_daemon_check::run_only_dual(&Options {
        source_policy: SourcePolicy::RetiredDriving,
        binary: daemon.clone(),
        catalog: required("DRIVING_CURRENT_CATALOG"),
        original_dir: fixture.original_directory(),
        original_manifest_sha256: fixture.original_manifest_sha256().to_owned(),
        collector: None,
        output: output.clone(),
        root,
        peer: None,
        compiler: Some(PathBuf::from("/usr/bin/g++")),
    });
    stages.record(
        "run-only-dual-return",
        result.as_ref().err().map(ToString::to_string).as_deref(),
    );
    let (strict_pass, error) = match result {
        Ok(report) => {
            assert_eq!(report["runs"].as_array().unwrap().len(), 1);
            assert_eq!(report["runs"][0]["frames"], 12);
            assert_eq!(report["runs"][0]["messages"], 33);
            (true, None)
        }
        Err(error) => {
            // exact 1차 거부와 새 packet의 실제 전체 차이를 함께 확인한다.
            assert_eq!(
                error.to_string(),
                "raw predictions differ from original model bytes"
            );
            (false, Some(error.to_string()))
        }
    };
    stages.mark("packet-open-before");
    let packet_path = output.join("1344-dual/modelV2-output-3.bin");
    let packet = DataSeal::open(&packet_path, 4_000_000);
    stages.mark("packet-open-after");
    stages.mark("packet-decode-before");
    let (frame, raw) = raw_packet(&fs::read(&packet_path).unwrap());
    assert_eq!(frame, 3);
    stages.mark("packet-decode-after");
    stages.mark("whole-raw-diff-before");
    let differences: Vec<_> = raw
        .chunks_exact(4)
        .zip(frozen.chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, (actual, expected))| {
            json!({"index":index,
            "actual_bits":format!("{:08x}",u32::from_le_bytes(actual.try_into().unwrap())),
            "frozen_bits":format!("{:08x}",u32::from_le_bytes(expected.try_into().unwrap()))})
        })
        .collect();
    stages.mark("whole-raw-diff-after");
    stages.mark("input-guards-before");
    for guard in [&request, &admission, &packet] {
        guard.verify();
    }
    stages.mark("input-guards-after");
    stages.mark("fixture-verify-before");
    fixture.verify().unwrap();
    stages.mark("fixture-verify-after");
    stages.mark("original-verify-before");
    original.verify().unwrap();
    stages.mark("original-verify-after");
    stages.mark("current-verify-before");
    current.verify();
    stages.mark("current-verify-after");
    stages.mark("daemon-verify-before");
    held.verify(&daemon);
    stages.mark("daemon-verify-after");
    stages.mark("receipt-write-before");
    let report = json!({"schema":1,"source":value["source"],"strict_pass":strict_pass,
        "strict_failure_sticky":!strict_pass,"error":error,"frame_id":frame,
        "all_words":2576,"differences":differences,"packet":packet.pin,
        "raw_sha256":format!("{:x}",Sha256::digest(&raw)),"frozen_raw_sha256":RAW,
        "later_recursive_fields_qualified":strict_pass,"diagnostic_complete":true,
        "full128_acceptance":false,"historical_half_snan5_resolved":false,"retirement_credit":0});
    let path = job.join("proof/model482/d1.json");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &report).unwrap();
    stages.mark("receipt-write-after");
    stages.finish();
    assert_eq!(
        differences.is_empty(),
        strict_pass,
        "strict error must match actual whole raw delta"
    );
}
