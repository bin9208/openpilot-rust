//! #482 Actions 전용 D1. 기존 strict 실패는 관측 후에도 성공으로 승격하지 않는다.
use super::*;
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::driving_daemon_check::{self, source_policy::SourcePolicy, Options};
use std::{io::Cursor, os::unix::fs::OpenOptionsExt};

const FIXTURE: &str = "afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d";
const RAW: &str = "9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe";

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
    let fixture = Fixture::open(&required("DRIVING_FIXTURE_DIRECTORY"), FIXTURE).unwrap();
    let current = CurrentCatalog::open(&root, &fixture);
    let original = Original::open(
        &fixture.original_directory(),
        fixture.original_manifest_sha256(),
    )
    .unwrap();
    let frozen = original.read("1344-dual/outputs/raw-pred-0.bin").unwrap();
    assert_eq!(frozen.len(), 10304);
    assert_eq!(format!("{:x}", Sha256::digest(&frozen)), RAW);
    let daemon = required("DRIVING_DAEMON_NATIVE_BINARY");
    let held = Held::open(&daemon);
    assert_eq!(value["daemon_sha256"], sha(&daemon));
    let output = job.join("build/model482/d1");
    assert!(!output.exists() && !output.is_symlink());
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
    let packet_path = output.join("1344-dual/modelV2-output-3.bin");
    let packet = DataSeal::open(&packet_path, 4_000_000);
    let (frame, raw) = raw_packet(&fs::read(&packet_path).unwrap());
    assert_eq!(frame, 3);
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
    for guard in [&request, &admission, &packet] {
        guard.verify();
    }
    fixture.verify().unwrap();
    original.verify().unwrap();
    current.verify();
    held.verify(&daemon);
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
    assert_eq!(
        differences.is_empty(),
        strict_pass,
        "strict error must match actual whole raw delta"
    );
}
