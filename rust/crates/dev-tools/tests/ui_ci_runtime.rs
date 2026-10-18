use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
    ui_ci_runtime::{run_evidence, vision_peer, DisplayPolicy, OwnedDisplay},
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs, io,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};
#[test]
fn original_peer_selection_cases_preserve_current_receipt_and_failures() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-ci-runtime/orchestration.json")).unwrap();
    for row in fixture["vision_cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let b = t.path();
        let current = b.join("current");
        fs::create_dir(&current).unwrap();
        fs::write(
            current.join("native-vision-peer"),
            b"not executable but file accepted",
        )
        .unwrap();
        symlink(&current, b.join("alias")).unwrap();
        let record = json!({"reason":"build-script-executed","package_id":"path+fixture#openpilot-msgq@0.1.0","out_dir":current});
        let name = row["case"].as_str().unwrap();
        let mut records = vec![record.clone()];
        match name {
            "current" => {}
            "dedup-alias" => {
                let mut r = record.clone();
                r["out_dir"] = json!(b.join("alias"));
                records.push(r)
            }
            "absent" => records.clear(),
            "ambiguous" => {
                let mut r = record.clone();
                r["out_dir"] = json!(b.join("other"));
                records.push(r)
            }
            "compiler-artifact" => records[0]["reason"] = json!("compiler-artifact"),
            "null" => records[0] = Value::Null,
            "missing-package" => {
                records[0].as_object_mut().unwrap().remove("package_id");
            }
            "package-number" => records[0]["package_id"] = json!(1),
            "outdir-null" => records[0]["out_dir"] = Value::Null,
            "nonmatching-package" => records[0]["package_id"] = json!("openpilot-msgq@0.1.0"),
            "malformed" | "blank" => {}
            _ => panic!("{name}"),
        }
        let text = if name == "malformed" {
            "{".into()
        } else if name == "blank" {
            String::new()
        } else {
            records
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let messages = b.join("messages");
        fs::write(&messages, text).unwrap();
        let result = vision_peer(&messages, b);
        assert_eq!(result.is_ok(), row["error"].is_null(), "{name}: {result:?}");
        if let Ok(path) = result {
            assert_eq!(path, current.join("native-vision-peer"));
        }
    }
}
#[test]
fn actual_collector_preserves_original_stream_decoding_and_failure_artifacts() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-ci-runtime/collector.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let name = row["case"].as_str().unwrap();
        let script = match name {
            "success" => "printf 'out\\r\\n';printf 'err\\r' >&2",
            "failure" => "printf 'out\\n';printf 'err\\n' >&2;exit 7",
            "invalid-utf8" => "printf '\\377'",
            "invalid-stderr" => "printf 'valid\\n';printf '\\377' >&2",
            "nonzero-invalid" => "printf '\\377';exit 7",
            "spawn" => "",
            _ => panic!(),
        };
        let argv: Vec<OsString> = if name == "spawn" {
            vec![t.path().join("missing").into()]
        } else {
            vec!["/bin/sh".into(), "-c".into(), script.into()]
        };
        let evidence = t.path().join("evidence");
        let mut printed = vec![];
        let mut errors = vec![];
        let result = run_evidence(
            &argv,
            t.path(),
            &evidence,
            "probe",
            &mut NativeRunner,
            &mut printed,
            &mut errors,
            Duration::from_secs(3),
        );
        assert_eq!(result.is_ok(), row["error"].is_null(), "{name}: {result:?}");
        assert_eq!(String::from_utf8(printed).unwrap(), row["printed_stdout"]);
        assert_eq!(String::from_utf8(errors).unwrap(), row["printed_stderr"]);
        let actual = fs::read_dir(&evidence)
            .unwrap()
            .map(|p| p.unwrap().file_name().into_string().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        let expected = row["files"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(actual, expected, "{name}");
        for file in ["probe.stdout", "probe.stderr", "probe.exit.json"] {
            if let Some(hex) = row["files"].get(file) {
                let bytes = fs::read(evidence.join(file)).unwrap();
                assert_eq!(
                    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                    hex.as_str().unwrap(),
                    "{name}/{file}"
                );
            }
        }
    }
}
fn display_script(root: &Path, body: &str) -> std::path::PathBuf {
    // Concurrent test spawns can briefly inherit another thread's open script
    // writer across fork. Exec then returns ETXTBSY despite fs::write closing
    // its own descriptor. Execute an immutable launcher and only read the body.
    fs::write(root.join("Xvfb.body"), body).unwrap();
    let script = root.join("Xvfb");
    symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-ci-runtime/Xvfb"),
        &script,
    )
    .unwrap();
    script
}
fn display_program(root: &Path, mode: &str) -> std::path::PathBuf {
    let body = format!(
        "#!/bin/bash\nprintf '%s' $$ > '{}'/pid\nfd=$2\ncase '{}' in\nexit) exit 3;;\ninvalid) eval \"printf 'bad\\\\n' >&$fd\";;\nunicode) eval \"printf '١٢\\\\n' >&$fd\";;\npartial) eval \"printf '4' >&$fd\";;\ntimeout) :;;\nignore-term) trap '' TERM; eval \"printf '43\\\\n' >&$fd\";;\n*) eval \"printf '43\\\\n' >&$fd\";;\nesac\nwhile :;do sleep .05;done\n",
        root.display(),
        mode
    );
    display_script(root, &body)
}
#[test]
fn display_peer_exec_does_not_depend_on_mutable_script_write_descriptors() {
    let t = tempfile::tempdir().unwrap();
    let script = display_program(t.path(), "success");
    let body = t.path().join("Xvfb.body");
    fs::set_permissions(&body, fs::Permissions::from_mode(0o755)).unwrap();
    // Model a writer inherited by an unrelated concurrent fork. Linux refuses
    // direct execution until every such descriptor is closed.
    let writer = fs::OpenOptions::new().append(true).open(&body).unwrap();
    let error = std::process::Command::new(&body).spawn().unwrap_err();
    assert_eq!(error.raw_os_error(), Some(26));
    let mut display = OwnedDisplay::start(
        script.as_os_str(),
        t.path(),
        DisplayPolicy {
            startup: Duration::from_secs(1),
            terminate: Duration::from_millis(100),
            line_limit: 65536,
        },
    )
    .unwrap();
    assert_eq!(display.display(), ":43");
    display.finish().unwrap();
    drop(writer);
}
#[test]
fn original_owned_display_contracts_and_explicit_partialline_guard() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/ui-ci-runtime/display.json")).unwrap();
    for row in original["display_cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let mode = row["case"].as_str().unwrap();
        let output = t.path().join("output");
        fs::create_dir(&output).unwrap();
        let script = display_program(t.path(), mode);
        let start = Instant::now();
        let result = OwnedDisplay::start(
            script.as_os_str(),
            &output,
            DisplayPolicy {
                startup: Duration::from_secs(1),
                terminate: Duration::from_millis(100),
                line_limit: 65536,
            },
        );
        assert_eq!(
            result.is_ok(),
            !row["yielded"].is_null(),
            "{mode}: error={:?}; xvfb.log={:?}; pid={:?}",
            result.as_ref().err().map(ToString::to_string),
            fs::read(t.path().join("output/xvfb.log")).ok(),
            fs::read_to_string(t.path().join("pid")).ok()
        );
        if let Ok(mut display) = result {
            assert_eq!(display.display(), row["yielded"].as_str().unwrap());
            assert!(display.pid() > 0);
            let receipt_bytes = fs::read(output.join("display.json")).unwrap();
            assert!(receipt_bytes.is_ascii());
            let actual: Value = serde_json::from_slice(&receipt_bytes).unwrap();
            let mut expected = row["receipt"].clone();
            expected["argv"][0] = json!(script);
            expected["argv"][2] = actual["argv"][2].clone();
            expected["pid"] = json!(display.pid());
            assert_eq!(actual, expected);
            let fd = actual["argv"][2].as_str().unwrap();
            assert!(
                std::process::Command::new("/bin/sh")
                    .args(["-c", "test ! -e /proc/self/fd/$1", "fd-check", fd])
                    .status()
                    .unwrap()
                    .success(),
                "displayfd leaked into unrelated child"
            );

            if mode == "body-failure" {
                drop(display);
            } else {
                display.finish().unwrap();
            }
        }
        let pid: i32 = fs::read_to_string(t.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "{mode} left child"
        );
        assert_eq!(
            output.join("display.json").exists(),
            !row["receipt"].is_null()
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }
    let t = tempfile::tempdir().unwrap();
    let script = display_program(t.path(), "partial");
    let started = Instant::now();
    assert!(OwnedDisplay::start(
        script.as_os_str(),
        t.path(),
        DisplayPolicy {
            startup: Duration::from_millis(100),
            terminate: Duration::from_millis(100),
            line_limit: 65536
        }
    )
    .is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
}
struct Refuse;
impl Runner for Refuse {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        Err(io::Error::other("controlled").into())
    }
}
#[test]
fn command_receipt_precedes_spawn_failure() {
    let t = tempfile::tempdir().unwrap();
    let result = run_evidence(
        &["missing".into()],
        t.path(),
        t.path(),
        "probe",
        &mut Refuse,
        &mut vec![],
        &mut vec![],
        Duration::from_secs(1),
    );
    assert!(result.is_err());
    assert!(t.path().join("probe.command.json").exists());
}
#[test]
fn independently_captured_display_unicode_whitespace_and_malformed_bytes() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-ci-runtime/display-text.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let payload = row["payload_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|v| format!("\\x{}", std::str::from_utf8(v).unwrap()))
            .collect::<String>();
        let script = display_script(
            t.path(),
            &format!(
                "#!/bin/bash\nfd=$2\neval \"printf '{payload}' >&$fd\"\nwhile :;do sleep .05;done\n"
            ),
        );
        let result = OwnedDisplay::start(
            script.as_os_str(),
            t.path(),
            DisplayPolicy {
                startup: Duration::from_secs(1),
                terminate: Duration::from_millis(100),
                line_limit: 65536,
            },
        );
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: error={:?}; xvfb.log={:?}; pid={:?}",
            row["case"],
            result.as_ref().err().map(ToString::to_string),
            fs::read(t.path().join("xvfb.log")).ok(),
            fs::read_to_string(t.path().join("pid")).ok()
        );
        if let Ok(mut display) = result {
            assert_eq!(display.display(), row["yielded"].as_str().unwrap());
            display.finish().unwrap();
        }
        assert_eq!(
            t.path().join("display.json").exists(),
            row["receipt"].as_bool().unwrap()
        );
    }
}
#[test]
fn explicit_collector_deadline_leaves_no_false_stream_or_exit_receipts() {
    let t = tempfile::tempdir().unwrap();
    let result = run_evidence(
        &[
            "/bin/sh".into(),
            "-c".into(),
            "printf before;sleep 2".into(),
        ],
        t.path(),
        t.path(),
        "probe",
        &mut NativeRunner,
        &mut vec![],
        &mut vec![],
        Duration::from_millis(80),
    );
    assert!(result.is_err());
    assert!(t.path().join("probe.command.json").exists());
    for name in ["probe.stdout", "probe.stderr", "probe.exit.json"] {
        assert!(!t.path().join(name).exists());
    }
}
#[test]
fn original_default_display_waits_are_preserved() {
    let p = DisplayPolicy::default();
    assert_eq!(p.startup, Duration::from_secs(15));
    assert_eq!(p.terminate, Duration::from_secs(10));
    assert_eq!(p.line_limit, 65536);
}
#[test]
#[ignore = "requires explicitly provisioned private Xvfb; never attaches to a user display"]
fn actual_private_xvfb_is_owned_and_reaped() {
    let binary = std::env::var_os("UI_CI_XVFB").expect("set private Xvfb path");
    let t = tempfile::tempdir().unwrap();
    let mut display = OwnedDisplay::start(&binary, t.path(), DisplayPolicy::default()).unwrap();
    let pid = display.pid();
    let output = std::process::Command::new("xdpyinfo")
        .args(["-display", display.display(), "-ext", "XTEST"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("XTEST"));
    if let Some(evidence) = std::env::var_os("UI_CI_ACTUAL_EVIDENCE") {
        let evidence = Path::new(&evidence);
        fs::create_dir_all(evidence).unwrap();
        for name in ["display.json", "xvfb.log"] {
            fs::copy(t.path().join(name), evidence.join(name)).unwrap();
        }
        fs::write(evidence.join("xdpyinfo.stdout"), &output.stdout).unwrap();
        fs::write(evidence.join("xdpyinfo.stderr"), &output.stderr).unwrap();
        fs::write(
            evidence.join("result.json"),
            format!(
                "{{\"owned_pid\": {pid}, \"display\": \"{}\", \"xtest\": true}}\n",
                display.display()
            ),
        )
        .unwrap();
    }
    display.finish().unwrap();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    if let Some(evidence) = std::env::var_os("UI_CI_ACTUAL_EVIDENCE") {
        fs::write(
            Path::new(&evidence).join("reaped.json"),
            format!("{{\"pid\": {pid}, \"reaped\": true}}\n"),
        )
        .unwrap();
    }
    println!("owned private Xvfb {pid} stopped and reaped");
}
#[test]
fn original_raw_paths_unused_nan_and_container_package_membership() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/ui-ci-runtime/peer-extra.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let base = t.path();
        for name in [
            OsString::from("current"),
            OsString::from_vec(b"raw-\xff".to_vec()),
        ] {
            let dir = base.join(name);
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("native-vision-peer"), b"file").unwrap();
        }
        fs::create_dir_all(base.join("directory/native-vision-peer")).unwrap();
        let messages = base.join("messages");
        fs::write(
            &messages,
            row["input_json"]
                .as_str()
                .unwrap()
                .replace("$BASE", base.to_str().unwrap()),
        )
        .unwrap();
        let result = vision_peer(&messages, base);
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["case"]
        );
        if let Ok(path) = result {
            let suffix = path.strip_prefix(base).unwrap().as_os_str().as_bytes();
            let mut normalized = b"$BASE/".to_vec();
            normalized.extend(suffix);
            assert_eq!(
                normalized
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                row["result_hex"].as_str().unwrap()
            );
        }
    }
}
#[test]
fn inherited_stdin_display_peer_entry() {
    if let Some(root) = std::env::var_os("UI_CI_STDIN_PEER_ROOT") {
        let root = Path::new(&root);
        let mut display = OwnedDisplay::start(
            root.join("Xvfb").as_os_str(),
            root,
            DisplayPolicy::default(),
        )
        .unwrap();
        assert_eq!(display.display(), ":43");
        display.finish().unwrap();
    }
}
#[test]
fn displayfd_does_not_repurpose_stdin_or_either_log_stream() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let t = tempfile::tempdir().unwrap();
    display_script(t.path(), &format!("#!/bin/bash\nfd=$2\n[ \"$fd\" -gt 2 ] || exit 5\nIFS= read -r input\nprintf '%s' \"$input\" > '{}/input'\nprintf 'out\\377'\nprintf 'err\\376' >&2\neval \"printf '43\\\\n' >&$fd\"\nwhile :;do sleep .05;done\n",t.path().display()));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "inherited_stdin_display_peer_entry",
            "--nocapture",
        ])
        .env("UI_CI_STDIN_PEER_ROOT", t.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"owned stdin\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(t.path().join("input")).unwrap(), b"owned stdin");
    assert_eq!(
        fs::read(t.path().join("xvfb.log")).unwrap(),
        b"out\xfferr\xfe"
    );
}
#[test]
fn inherited_stdin_collector_peer_entry() {
    if let Some(root) = std::env::var_os("UI_CI_COLLECTOR_STDIN_ROOT") {
        let root = Path::new(&root);
        run_evidence(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "cat;printf stderr\\r >&2".into(),
            ],
            root,
            root,
            "probe",
            &mut NativeRunner,
            &mut vec![],
            &mut vec![],
            Duration::from_secs(3),
        )
        .unwrap();
    }
}
#[test]
fn collector_preserves_real_inherited_stdin() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let t = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "inherited_stdin_collector_peer_entry",
            "--nocapture",
        ])
        .env("UI_CI_COLLECTOR_STDIN_ROOT", t.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"owned input\r\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(t.path().join("probe.stdout")).unwrap(),
        b"owned input\n"
    );
}
