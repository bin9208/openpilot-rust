#![cfg(target_os = "linux")]

use openpilot_dev_tools::route_upload_check::{
    compare_transfers, runtime_crash, stats_records, Request,
};
use serde_json::json;

#[test]
fn explicit_runtime_root_preserves_the_original_compiled_default() {
    use openpilot_dev_tools::route_upload_check::parse_options;
    use std::ffi::OsString;
    let temporary = tempfile::tempdir().unwrap();
    let args = vec![
        OsString::from("--binaries"),
        temporary.path().join("bin").into_os_string(),
        OsString::from("--output"),
        temporary.path().join("output").into_os_string(),
    ];
    let original_default = parse_options(false, &args).unwrap().unwrap();
    let expected = openpilot_dev_tools::path_resolve::resolve_non_strict(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
    )
    .unwrap();
    assert_eq!(original_default.root, expected);
    let mut explicit = args;
    explicit.extend([OsString::from("--root"), temporary.path().into()]);
    let selected = parse_options(false, &explicit).unwrap().unwrap();
    assert_eq!(selected.root, temporary.path());
    assert_eq!(selected.binaries, original_default.binaries);
    assert_eq!(selected.output, original_default.output);
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[test]
fn required_options_precede_unknown_argument_diagnostics_like_argparse() {
    use openpilot_dev_tools::route_upload_check::parse_options;
    for manager in [false, true] {
        let error = parse_options(manager, &["--not-an-option".into()]).unwrap_err();
        assert!(error.to_string().contains("required"));
        assert!(!error.to_string().contains("unrecognized"));
        assert!(
            parse_options(manager, &["--not-an-option".into(), "--help".into()])
                .unwrap()
                .is_none()
        );
    }
}

fn crash() -> serde_json::Value {
    json!({"msg":"crash","ctx":{"runtime_language":"rust","runtime_implementation":"openpilot-uploader","daemon":"uploader","dongle_id":"owned-device","source_commit":"a".repeat(40),"source_tree":"clean"},"exc_info":"openpilot_uploader::Error"})
}
#[test]
fn runtime_crash_rejects_missing_duplicate_and_wrong_identity() {
    let good = crash();
    assert!(runtime_crash(std::slice::from_ref(&good)).is_ok());
    assert!(runtime_crash(&[]).is_err());
    assert!(runtime_crash(&[good.clone(), good.clone()]).is_err());
    for (name, value) in [
        ("runtime_language", json!("python")),
        ("runtime_implementation", json!("statsd-rs")),
        ("daemon", json!("statsd")),
        ("dongle_id", json!("other")),
        ("source_commit", json!("a".repeat(39))),
        ("source_commit", json!("G".repeat(40))),
        ("source_tree", json!("unknown")),
    ] {
        let mut bad = good.clone();
        bad["ctx"][name] = value;
        assert!(runtime_crash(&[bad]).is_err(), "{name}");
    }
    let mut bad = good;
    bad["exc_info"] = json!("unrelated failure");
    assert!(runtime_crash(&[bad]).is_err());
}
#[test]
fn manager_records_reject_missing_wrong_source_and_duplicate_crash() {
    let normal = json!({"ctx":{"daemon":"statsd","runtime_implementation":"statsd-rs","runtime_language":"rust","commit":"fixture"}});
    let crash = json!({"msg":"crash","ctx":{"daemon":"statsd","runtime_implementation":"statsd-rs","runtime_language":"rust"}});
    assert_eq!(
        stats_records(
            std::slice::from_ref(&normal),
            std::slice::from_ref(&crash),
            "fixture"
        )
        .unwrap(),
        (1, 1)
    );
    assert!(stats_records(&[], std::slice::from_ref(&crash), "fixture").is_err());
    assert!(stats_records(std::slice::from_ref(&normal), &[], "fixture").is_err());
    assert!(stats_records(
        std::slice::from_ref(&normal),
        &[crash.clone(), crash.clone()],
        "fixture"
    )
    .is_err());
    assert!(stats_records(&[normal], std::slice::from_ref(&crash), "wrong").is_err());
}
fn exchanges() -> Vec<Request> {
    vec![
        Request {
            method: "GET".into(),
            path: "/v1.4/000/upload_url/?path=route--0%2Fqlog.zst".into(),
            body: vec![],
        },
        Request {
            method: "PUT".into(),
            path: "/put".into(),
            body: b"route-bytes".to_vec(),
        },
    ]
}
#[test]
fn transfers_reject_odd_order_wrong_path_and_changed_body() {
    let rows = exchanges();
    assert_eq!(
        compare_transfers(&rows, "route--0/qlog.zst", b"route-bytes")
            .unwrap()
            .len(),
        1
    );
    assert!(compare_transfers(&rows[..1], "route--0/qlog.zst", b"route-bytes").is_err());
    assert!(compare_transfers(&rows, "other/qlog.zst", b"route-bytes").is_err());
    assert!(compare_transfers(&rows, "route--0/qlog.zst", b"changed").is_err());
    let mut wrong = exchanges();
    wrong.swap(0, 1);
    assert!(compare_transfers(&wrong, "route--0/qlog.zst", b"route-bytes").is_err());
    assert!(compare_transfers(&[], "route--0/qlog.zst", b"route-bytes").is_err());
}
#[test]
fn full_schema_route_decoder_rejects_truncated_data() {
    use openpilot_dev_tools::route_upload_check::route_records;
    assert!(route_records(b"not-zstd", "errorLogMessage").is_err());
    let mut message = capnp::message::Builder::new_default();
    message
        .init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>()
        .set_error_log_message("{\"msg\":\"crash\"}");
    let mut raw = Vec::new();
    capnp::serialize::write_message(&mut raw, &message).unwrap();
    let mut other = capnp::message::Builder::new_default();
    other
        .init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>()
        .init_car_state()
        .set_v_ego(12.5);
    capnp::serialize::write_message(&mut raw, &other).unwrap();
    let packed = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
    assert_eq!(route_records(&packed, "errorLogMessage").unwrap().0, 2);
    assert_eq!(
        route_records(&packed, "errorLogMessage").unwrap().1,
        vec![json!({"msg":"crash"})]
    );
    raw.pop();
    let truncated = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
    assert!(route_records(&truncated, "errorLogMessage").is_err());
}

#[test]
fn cli_admission_rejects_errors_before_opening_output() {
    use std::process::Command;
    for binary in [
        env!("CARGO_BIN_EXE_check-manager-log-upload"),
        env!("CARGO_BIN_EXE_check-runtime-log-upload"),
    ] {
        for args in [vec!["--help"], vec!["-h"]] {
            let reply = Command::new(binary).args(args).output().unwrap();
            assert!(reply.status.success());
            assert!(reply.stdout.starts_with(b"usage: "));
            assert!(reply.stderr.is_empty());
        }
        for args in [
            vec![],
            vec!["--binaries"],
            vec!["--unknown"],
            vec!["--output=fixture"],
            vec!["--help=value"],
            vec!["--", "--help"],
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let reply = Command::new(binary)
                .current_dir(temporary.path())
                .args(args)
                .output()
                .unwrap();
            assert_eq!(reply.status.code(), Some(2));
            assert!(reply.stdout.is_empty());
            assert!(!reply.stderr.is_empty());
            assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
        }
    }
}

#[test]
fn runtime_missing_provider_fails_without_success_receipt() {
    let temporary = tempfile::tempdir().unwrap();
    let binaries = temporary.path().join("missing-binaries");
    let output = temporary.path().join("output");
    std::fs::create_dir(&binaries).unwrap();
    let reply = std::process::Command::new(env!("CARGO_BIN_EXE_check-runtime-log-upload"))
        .arg("--binaries")
        .arg(&binaries)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(reply.status.code(), Some(1));
    assert!(reply.stdout.is_empty());
    assert!(!output.join("result.json").exists());
    assert!(output.join("openpilot-logmessaged.log").is_file());
    assert!(!output.join("openpilot-loggerd.log").exists());
    assert!(!output.join("uploader.log").exists());
}

#[test]
fn both_controllers_reject_an_existing_output_without_clobbering() {
    for (binary, manager) in [
        (env!("CARGO_BIN_EXE_check-manager-log-upload"), true),
        (env!("CARGO_BIN_EXE_check-runtime-log-upload"), false),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let foreign = output.join("openpilot-logmessaged.log");
        std::fs::write(&foreign, b"existing foreign diagnostic\n").unwrap();
        let mut command = std::process::Command::new(binary);
        command
            .arg("--binaries")
            .arg(temporary.path().join("missing-binaries"))
            .arg("--output")
            .arg(&output);
        if manager {
            command
                .arg("--root")
                .arg(temporary.path())
                .arg("--probe")
                .arg(temporary.path().join("missing-probe"));
        }
        let reply = command.output().unwrap();
        assert_eq!(reply.status.code(), Some(1));
        assert!(reply.stdout.is_empty());
        assert_eq!(
            std::fs::read(&foreign).unwrap(),
            b"existing foreign diagnostic\n"
        );
        assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
    }
}
#[test]
fn manager_command_receipt_preserves_non_utf8_paths() {
    use openpilot_log_format::JsonValue;
    use std::os::unix::ffi::OsStringExt;
    let argument = std::ffi::OsString::from_vec(b"/tmp/path-\xff".to_vec());
    let encoded =
        openpilot_dev_tools::route_upload_check::command_json(std::slice::from_ref(&argument))
            .unwrap();
    assert_eq!(encoded, "[\"/tmp/path-\\udcff\"]");
    let parsed = JsonValue::parse(&encoded).unwrap();
    let openpilot_log_format::JsonView::Array(values) = parsed.view() else {
        panic!("argument array missing")
    };
    assert_eq!(
        openpilot_dev_tools::python_fs::from_json(&values[0]).unwrap(),
        argument
    );
}
