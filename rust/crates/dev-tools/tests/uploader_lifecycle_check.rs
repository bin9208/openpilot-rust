#![cfg(feature = "uploader-siblings-native")]
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};
fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("mandatory {key}"))
}
fn spec(scope: &str) -> CommandSpec {
    let mut spec = CommandSpec::new(
        vec![
            env("UPLOADER_SIBLING_CHECKER"),
            "--root".into(),
            env("OPENPILOT_RUST_ROOT"),
            "--scope".into(),
            scope.into(),
            "--fixture".into(),
            env("UPLOADER_SIBLING_ORIGINAL"),
            "--fixture-sha256".into(),
            "30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176".into(),
            "--output".into(),
            PathBuf::from(env("UPLOADER_SIBLING_PROOF"))
                .join(scope)
                .to_string_lossy()
                .into_owned(),
        ],
        Duration::from_secs(300),
    );
    for (flag, key) in [
        ("--binary", "UPLOADER_BINARY"),
        ("--trace", "UPLOADER_TRACE_BINARY"),
        ("--fault", "UPLOADER_FAULT_BINARY"),
        ("--collector", "UPLOADER_COLLECTOR_BINARY"),
        ("--interrupted", "UPLOADER_INTERRUPTED_BINARY"),
        ("--http-provider", "UPLOADER_HTTP_PROVIDER"),
    ] {
        spec.argv.extend([flag.into(), env(key)]);
    }
    spec
}
fn run(scope: &str) -> Value {
    let spec = spec(scope);
    let captured = NativeRunner.run(&spec).unwrap();
    let output = PathBuf::from(env("UPLOADER_SIBLING_PROOF")).join(scope);
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("cli.stdout"), &captured.stdout).unwrap();
    fs::write(output.join("cli.stderr"), &captured.stderr).unwrap();
    assert_eq!(
        captured.code,
        0,
        "scope {scope}: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let value: Value = serde_json::from_slice(&captured.stdout).unwrap();
    assert_eq!(value["passed"], true);
    assert_eq!(value["fresh_python_runtime"], false);
    value
}
#[test]
fn corrupted_original_observations_are_rejected_before_native_providers() {
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("corrupt.json");
    fs::write(&bad, b"{}").unwrap();
    let mut command = spec("cleanup");
    let index = command.argv.iter().position(|v| v == "--fixture").unwrap();
    command.argv[index + 1] = bad.to_string_lossy().into_owned();
    let output = NativeRunner.run(&command).unwrap();
    assert_ne!(output.code, 0);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sibling original observations changed")
    );
}
#[test]
fn six_actual_daemon_cases_and_three_startup_failures() {
    let v = run("daemon");
    assert_eq!(
        v["reports"]["daemon"]["scenarios"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        v["reports"]["daemon"]["startup_errors"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn interrupted_send_retries_preserve_actual_packet_and_delivery() {
    let v = run("interrupted-send");
    assert_eq!(v["reports"]["interrupted-send"]["interrupted_attempts"], 3);
}
#[test]
fn interrupted_http_read_retains_two_real_uploads() {
    run("interrupted-http");
}
#[test]
fn ten_socket_deadline_and_progress_cases_match_original() {
    let v = run("timeouts");
    assert_eq!(v["reports"]["timeouts"]["executions"], 10);
}
#[test]
fn fifteen_fault_cases_cross_both_actual_collectors() {
    let v = run("fault");
    assert_eq!(v["reports"]["fault"]["native_executions"], 30);
}
#[test]
fn seven_typed_params_cases_cross_both_actual_collectors() {
    let v = run("params");
    assert_eq!(v["reports"]["params"]["native_executions"], 14);
}
#[test]
fn real_timeout_and_early_spawn_cleanup_preserve_unrelated_resources() {
    let v = run("cleanup");
    assert_eq!(
        v["reports"]["cleanup"]["scenarios"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn downstream_records_failure_retains_actual_exit_and_cleans_worker_namespace() {
    use openpilot_dev_tools::uploader_logging_check::normalized;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{
        ffi::OsStr,
        fs::File,
        io::Read,
        os::unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt},
        },
        path::Path,
    };
    fn bounded(path: &Path, limit: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        File::open(path)
            .unwrap()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() as u64 <= limit);
        bytes
    }
    fn os_value(value: &OsStr) -> Value {
        json!({"os_hex":value.as_bytes().iter().map(|v|format!("{v:02x}")).collect::<String>(),
            "utf8":value.to_str()})
    }

    let fixture = PathBuf::from(env("UPLOADER_SIBLING_ORIGINAL"));
    let bytes = bounded(&fixture, 32 << 20);
    let fixture_sha = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        fixture_sha,
        "30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176"
    );
    let corpus: Value = serde_json::from_slice(&bytes).unwrap();
    let rows = corpus["params"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["case"] == "missing-id" && row["original_collector"] == false)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let expected = rows[0];
    assert_eq!(expected["source"], true);
    assert_eq!(expected["exit"], 1);
    let original_records = expected["records"].as_array().unwrap();
    assert!(original_records
        .iter()
        .all(|row| row["msg"].get("fn").is_none()));
    let mut downstream = expected.clone();
    // 원본 파일/expected는 보존하고 private worker 요청만 사후 거부로 유도한다.
    downstream["records"] = Value::Null;
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("logs");
    fs::DirBuilder::new().mode(0o700).create(&work).unwrap();
    let metadata = fs::symlink_metadata(&work).unwrap();
    let token = temp
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .replace('.', "-");
    let prefix = format!("rust-probe-uploader-life-{}-{token}", std::process::id());
    let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
    assert!(matches!(fs::symlink_metadata(&shm),Err(e) if e.kind()==std::io::ErrorKind::NotFound));
    let proof = PathBuf::from(env("UPLOADER_SIBLING_PROOF"));
    fs::create_dir_all(&proof).unwrap();
    let output = proof.join("records-exit-downstream-failure");
    fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
    let root = PathBuf::from(env("OPENPILOT_RUST_ROOT"));
    let binary = PathBuf::from(env("UPLOADER_BINARY"));
    let worker = PathBuf::from(env("UPLOADER_SIBLING_CHECKER"));
    let request = output.join("request.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({"root":root,"binary":binary,
            "collector":env("UPLOADER_COLLECTOR_BINARY"),"worker":worker,
            "work":work,"work_identity":[metadata.dev(),metadata.ino()],
            "output":output,"prefix":prefix,"fault":false,"expected":downstream}))
        .unwrap(),
    )
    .unwrap();
    let mut command = CommandSpec::new_os(
        vec![
            worker.into_os_string(),
            "--uploader-records-worker".into(),
            request.into_os_string(),
        ],
        Duration::from_secs(75),
    );
    command.cwd = Some(root.clone());
    command
        .env
        .insert("OPENPILOT_PREFIX".into(), prefix.clone());
    // 이 worker가 직접 만든 namespace의 Drop을 확인하며 외부 owner를 빌리지 않는다.
    command.env_remove = [
        "UPLOADER_QA_NAMESPACE",
        "ROUTE_UPLOAD_QA_IPC_ROOT",
        "ZMQ",
        "CEREAL_FAKE",
        "LD_PRELOAD",
    ]
    .map(Into::into)
    .to_vec();
    let captured = NativeRunner.run(&command).unwrap();
    fs::write(output.join("worker.stdout"), &captured.stdout).unwrap();
    fs::write(output.join("worker.stderr"), &captured.stderr).unwrap();
    assert_eq!(captured.code, 1);
    assert_eq!(
        String::from_utf8_lossy(&captured.stderr)
            .lines()
            .filter(|line| *line == "check-uploader-siblings: original source records missing")
            .count(),
        1,
        "must reach the downstream records check, not a spawn/permission/peer failure"
    );
    let path = output.join("uploader-exit.json");
    let meta = fs::symlink_metadata(&path).unwrap();
    assert!(meta.is_file() && !meta.file_type().is_symlink());
    assert_eq!(meta.mode() & 0o7777, 0o600);
    assert_eq!(meta.nlink(), 1);
    let receipt: Value = serde_json::from_slice(&bounded(&path, 64 << 10)).unwrap();
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["capture"]["state"], "captured");
    assert_eq!(receipt["capture"]["exit_code"], 1);
    assert_eq!(
        receipt["capture"]["spawn_status"],
        "completed_native_capture"
    );
    assert_eq!(receipt["command_timeout_ms"], 30000);
    assert_eq!(
        receipt["argv"],
        json!([
            os_value(binary.as_os_str()),
            os_value(OsStr::new("--cycles")),
            os_value(OsStr::new("1"))
        ])
    );
    assert_eq!(receipt["cwd"], os_value(root.as_os_str()));
    assert_eq!(receipt["worker_host_context"]["os"], std::env::consts::OS);
    assert_eq!(
        receipt["worker_host_context"]["arch"],
        std::env::consts::ARCH
    );
    assert_eq!(
        receipt["capture"]["stdout_bytes"],
        bounded(&output.join("uploader.stdout"), 16 << 20).len()
    );
    assert_eq!(
        receipt["capture"]["stderr_bytes"],
        bounded(&output.join("uploader.stderr"), 16 << 20).len()
    );
    for name in ["collector.stdout", "collector.stderr"] {
        bounded(&output.join(name), 16 << 20);
    }
    let actual: Vec<Value> =
        serde_json::from_slice(&bounded(&output.join("records.json"), 32 << 20)).unwrap();
    assert_eq!(
        normalized(&actual, &work, None).unwrap(),
        normalized(original_records, Path::new("/"), None).unwrap()
    );
    let http: Vec<Value> =
        serde_json::from_slice(&bounded(&output.join("http.json"), 32 << 20)).unwrap();
    assert_eq!(
        http.len() as u64,
        expected["http_requests"].as_u64().unwrap()
    );
    assert!(
        matches!(fs::symlink_metadata(output.join("comparison.json")),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound)
    );
    assert_eq!(receipt["namespace"]["prefix"], prefix);
    let ipc = PathBuf::from(receipt["namespace"]["ipc_root"]["utf8"].as_str().unwrap());
    assert_eq!(ipc.parent(), Some(Path::new("/tmp")));
    assert!(ipc
        .file_name()
        .unwrap()
        .as_bytes()
        .starts_with(b"route-upload-ipc-"));
    for path in [&shm, &ipc] {
        assert!(
            matches!(fs::symlink_metadata(path),Err(e) if e.kind()==std::io::ErrorKind::NotFound)
        );
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(bounded(&fixture, 32 << 20))),
        fixture_sha,
        "original observations must remain immutable"
    );
}
