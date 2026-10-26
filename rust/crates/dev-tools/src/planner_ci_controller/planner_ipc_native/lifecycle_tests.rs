//! lifecycle 전용 CI 회귀. 원본 raw234/72와 실제 daemon26 양성 수용은 별도 Actions gate다.
use super::{
    corpus::RawFile,
    lifecycle,
    lifecycle_corpus::{self as corpus, CaseResult, HeldFile, Run, State, CASES},
    peer::Peer,
};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn original_thirteen_cases_keep_source_then_rust_raw_signal_and_fatal_exits() {
    // check_plannerd_lifecycle.py/source6c의 case table과 기존0283 실제26 결과에서 읽은 기대값이다.
    let expected = [
        ("wait-missing-sigint", -2, 0),
        ("wait-missing-sigterm", -2, 0),
        ("wait-directory-sigint", -2, 0),
        ("wait-directory-sigterm", -2, 0),
        ("wait-empty-sigint", -2, 0),
        ("wait-empty-sigterm", -2, 0),
        ("ready-sigint", -2, 0),
        ("ready-sigterm", -15, -15),
        ("recover-directory", -15, -15),
        ("settings-directories", -15, -15),
        ("fatal-integer", -6, 1),
        ("fatal-float", -6, 1),
        ("malformed-CarParams", 1, 1),
    ];
    let mut inputs = 0;
    for (case, (name, source, rust)) in CASES.iter().zip(expected) {
        assert_eq!(case.name, name);
        assert_eq!(
            (case.expected("source"), case.expected("rust")),
            (source, rust)
        );
        inputs += 2 * case.frames() * 9;
    }
    assert_eq!(inputs, 234);
    assert_eq!(corpus::SEND_ORDER[8], "modelV2");
    assert!(!corpus::SEND_ORDER.contains(&"liveTracks"));
}

#[test]
fn missing_actual_external_pins_reject_before_output_or_child() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    let options = lifecycle::Options {
        root: path.into(),
        python: path.join("must-not-start-python"),
        binary: path.join("must-not-start-daemon"),
        artifact: path.join("artifact"),
        binding: path.join("binding"),
        source_native: path.join("source"),
        original_ipc_binding: path.join("cpp-binding"),
        corpus: path.join("corpus"),
        corpus_audit: path.join("audit"),
        output: path.join("must-not-create"),
        timeout: Duration::from_secs(1),
        growth_bytes: 256 * corpus::MIB,
    };
    assert!(corpus::qualified_audit("unqualified-host").is_err());
    assert!(lifecycle::run(&options).is_err());
    assert_eq!(fs::read_dir(path).unwrap().count(), 0);
    // lib8은 feature를 켠 실제 strict parser도 검사한다. 값 생성/원본 실행은 하지 않는다.
    let mut arguments: Vec<std::ffi::OsString> = Vec::new();
    for (flag, value) in [
        ("--root", &options.root),
        ("--python", &options.python),
        ("--binary", &options.binary),
        ("--artifact", &options.artifact),
        ("--binding", &options.binding),
        ("--source-native", &options.source_native),
        ("--original-ipc-binding", &options.original_ipc_binding),
        ("--corpus", &options.corpus),
        ("--corpus-audit", &options.corpus_audit),
        ("--output", &options.output),
    ] {
        arguments.extend([flag.into(), value.as_os_str().to_owned()]);
    }
    arguments.extend([
        "--timeout-seconds".into(),
        "2700".into(),
        "--growth-mib".into(),
        "256".into(),
    ]);
    let parse = |arguments: Vec<std::ffi::OsString>| {
        crate::planner_ci_controller::cli::parse_lifecycle_arguments(arguments.into_iter())
    };
    let parsed = parse(arguments.clone()).unwrap();
    assert_eq!(parsed.python, options.python);
    assert_eq!(parsed.timeout, Duration::from_secs(2700));
    assert_eq!(parsed.growth_bytes, 256 * corpus::MIB);
    for (flag, value) in [
        ("--root", "relative"),
        ("--growth-mib", "0"),
        ("--growth-mib", "1.5"),
        ("--growth-mib", "18446744073709551615"),
        ("--timeout-seconds", "0"),
    ] {
        let mut invalid = arguments.clone();
        let index = invalid.iter().position(|v| v == flag).unwrap() + 1;
        invalid[index] = value.into();
        assert!(parse(invalid).is_err());
    }
    for extra in [vec!["--unknown", "x"], vec!["--growth-mib", "256"]] {
        let mut invalid = arguments.clone();
        invalid.extend(extra.into_iter().map(std::ffi::OsString::from));
        assert!(parse(invalid).is_err());
    }
    let index = arguments.iter().position(|v| v == "--binding").unwrap();
    arguments.drain(index..=index + 1);
    assert!(parse(arguments).is_err());
    assert_eq!(fs::read_dir(path).unwrap().count(), 0);
}

#[test]
fn schema1_rejects_unknown_incomplete_and_ambiguous_params_metadata() {
    assert!(serde_json::from_str::<Run>(r#"{"case":"ready-sigint","unknown":true}"#).is_err());
    assert!(serde_json::from_str::<State>(r#"{"kind":"dir","raw":{}}"#).is_err());
    assert!(serde_json::from_str::<State>(r#"{"kind":"symlink"}"#).is_err());
    assert!(serde_json::from_str::<State>(r#"{"kind":"file"}"#).is_err());
    assert!(matches!(
        serde_json::from_str::<State>(r#"{"kind":"absent"}"#).unwrap(),
        State::Absent { .. }
    ));
    assert!(matches!(
        serde_json::from_str::<State>(r#"{"kind":"dir"}"#).unwrap(),
        State::Directory { .. }
    ));
}

#[test]
fn observed_empty_param_file_stays_distinct_from_absent_and_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("empty");
    fs::write(&path, []).unwrap();
    let raw = RawFile {
        path: "empty".into(),
        bytes: 0,
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
    };
    let (bytes, held) = HeldFile::open(root.path(), &raw, 0, true).unwrap();
    assert!(bytes.is_empty());
    fs::rename(&path, root.path().join("old-inode")).unwrap();
    fs::write(&path, []).unwrap();
    assert!(held.verify().is_err());
    fs::remove_file(&path).unwrap();
    symlink(root.path().join("old-inode"), &path).unwrap();
    assert!(HeldFile::open(root.path(), &raw, 0, true).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(HeldFile::open(root.path(), &raw, 0, true).is_err());
}

#[test]
fn member_paths_and_bounds_reject_escape_alias_and_size_changes() {
    for path in [
        "",
        "../packet",
        "/packet",
        "a//packet",
        "a/./packet",
        "a/../packet",
        "a\\packet",
        "a\0packet",
    ] {
        assert!(!corpus::relative(path), "{path:?}");
    }
    assert!(corpus::relative("00-case-source/inputs/00-carState.bin"));
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("packet"), b"abc").unwrap();
    let raw = RawFile {
        path: "packet".into(),
        bytes: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
    };
    assert!(HeldFile::open(root.path(), &raw, 2, true).is_err());
    let (_, held) = HeldFile::open(root.path(), &raw, 3, true).unwrap();
    fs::write(root.path().join("packet"), b"abd").unwrap();
    assert!(held.verify().is_err());
}

#[test]
fn actual_daemon_spawn_inherits_worker_pgid_and_keeps_exit42_without_normalization() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 0.1; exit 42"]);
    let mut child = lifecycle::spawn_daemon(&mut command).unwrap();
    let stat = fs::read_to_string(format!("/proc/{}/stat", child.child.id())).unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    assert_eq!(
        fields[2],
        rustix::process::getpgrp()
            .as_raw_nonzero()
            .get()
            .to_string()
    );
    assert_eq!(
        lifecycle::wait(&mut child, Instant::now() + Duration::from_secs(3)).unwrap(),
        42
    );
    assert!(child.reaped);
    lifecycle::signal(&mut child, 2).unwrap();
    assert_eq!(lifecycle::poll(&mut child).unwrap(), Some(42));
    assert!(!Path::new("/proc")
        .join(child.child.id().to_string())
        .exists());
    let mut absent = Command::new("/missing-lifecycle-daemon-must-not-start");
    assert!(lifecycle::spawn_daemon(&mut absent).is_err());
}

#[test]
fn native_peer_eof_and_wrong_ack_fail_closed_and_preserve_first_error() {
    for body in ["exit 42", "read request; printf 'WRONG\\n'; read request"] {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("peer");
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf 'READY\\n'\n{body}\n"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut peer = Peer::start(
            &executable,
            "lifecycle-unit",
            &root.path().join("stderr"),
            deadline,
        )
        .unwrap();
        assert!(peer.ok("readers", deadline).is_err());
        assert_eq!(
            peer.finish::<()>(Err("first lifecycle failure42".into()))
                .unwrap_err()
                .to_string(),
            "first lifecycle failure42"
        );
    }
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("bad-start");
    fs::write(&executable, "#!/bin/sh\nprintf 'WRONG\\n'\nexit 42\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(Peer::start(
        &executable,
        "lifecycle-unit",
        &root.path().join("stderr"),
        Instant::now() + Duration::from_secs(3)
    )
    .is_err());
}

#[test]
fn case_receipt_retains_raw_status_and_has_no_new_valid_data_acceptance_fields() {
    let result = CaseResult {
        case: "fatal-integer".into(),
        mode: "source".into(),
        ok: true,
        publications: None,
        returncode: Some(-6),
        expected_returncode: Some(-6),
        error: None,
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["returncode"], -6);
    assert_eq!(value["expected_returncode"], -6);
    assert!(value.get("publications").is_none());
    assert!(value.get("valid").is_none());
    assert!(value.get("data").is_none());
    assert_eq!(serde_json::from_value::<CaseResult>(value).unwrap(), result);
}
