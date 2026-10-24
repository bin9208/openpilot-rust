use openpilot_dev_tools::{
    msgq_sanitizers_check,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
struct Fake {
    case: String,
    calls: Vec<Value>,
}
impl Runner for Fake {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let capture = spec.argv[0] == "cargo" || spec.argv[0] == "g++";
        assert_eq!(spec.inherit_stderr, capture);
        assert_eq!(spec.inherit_output, !capture);
        assert!(!spec.merge_output);
        assert!(spec.inherit_stdin);
        let mut call = json!({"argv":spec.argv,"cwd":spec.cwd.as_ref().map(|_|"$RUST"),"timeout":if capture {Value::Null} else {json!(spec.timeout.as_secs())},"capture_stdout":capture,"text":capture,"check":true});
        if spec.argv[0] != "g++" {
            let mut env = serde_json::Map::new();
            for k in [
                "CARGO_TARGET_DIR",
                "CXXFLAGS",
                "RUSTFLAGS",
                "LD_PRELOAD",
                "ASAN_OPTIONS",
                "UBSAN_OPTIONS",
                "CFLAGS",
                "SENTINEL",
            ] {
                let fallback = match k {
                    "LD_PRELOAD" | "ASAN_OPTIONS" | "UBSAN_OPTIONS" => Some("old"),
                    "CFLAGS" => Some("preserved"),
                    "SENTINEL" => Some("inherited"),
                    _ => None,
                };
                env.insert(
                    k.into(),
                    json!(spec.env.get(k).map(String::as_str).or(fallback)),
                );
            }
            call["env"] = Value::Object(env);
        }
        self.calls.push(call);
        if self.case == "spawn-failed" {
            return Err("controlled spawn failure".into());
        }
        let mut out = Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        };
        if spec.argv[0] == "cargo" {
            assert_eq!(
                spec.env.get("CARGO_INCREMENTAL").map(String::as_str),
                Some("0")
            );
            assert_eq!(
                spec.env.get("CARGO_BUILD_JOBS").map(String::as_str),
                Some("2")
            );
            let n = match self.case.as_str() {
                "count-three" => 3,
                "count-five" => 5,
                _ => 4,
            };
            let mut binaries = ["vision", "creation", "transport", "queued", "extra"]
                .iter()
                .take(n)
                .map(|s| json!(format!("/controlled/{s}")))
                .collect::<Vec<_>>();
            if self.case == "duplicate-four" {
                binaries = vec![json!("/controlled/vision"); 4];
            }
            if self.case == "nonstring" {
                binaries[1] = json!(true);
            }
            let mut lines = binaries
                .iter()
                .map(|b| json!({"reason":"compiler-artifact","executable":b}).to_string())
                .collect::<Vec<_>>();
            if self.case == "false-empty" {
                lines.push(r#"{"reason":"compiler-artifact","executable":[]}"#.into());
            }
            let sep = if self.case == "unicode-lines" {
                "\u{2028}"
            } else {
                "\r\n"
            };
            out.stdout = (lines.join(sep) + "\r").into_bytes();
            match self.case.as_str() {
                "malformed" => out.stdout = b"{bad".to_vec(),
                "nonobject" => out.stdout = b"[]".to_vec(),
                "blank-line" => out.stdout.extend_from_slice(b"\n\n"),
                "build-invalid-utf8" => out.stdout = vec![255],
                "build-failed" => out.code = 7,
                _ => (),
            }
        } else if spec.argv[0] == "g++" {
            out.stdout = if self.case == "lookup-whitespace" {
                "\u{1c}\u{2003}/controlled/libasan.so\u{1f}\r"
                    .as_bytes()
                    .to_vec()
            } else {
                b" \t/controlled/libasan.so\r\n".to_vec()
            };
            if self.case == "lookup-failed" {
                out.code = 9;
            }
            if self.case == "lookup-invalid-utf8" {
                out.stdout = vec![255];
            }
        } else {
            assert!(spec.inherit_output);
            if self.case == "run-second-failed" && self.calls.len() == 4 {
                out.code = 8;
            }
            if self.case == "run-timeout" {
                return Err("controlled timeout".into());
            }
        }
        Ok(out)
    }
}
#[test]
fn original_eighteen_cases_preserve_order_env_and_failure_boundaries() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/msgq-sanitizers/original.json")).unwrap();
    for row in oracle["cases"].as_array().unwrap() {
        let case = row["case"].as_str().unwrap();
        let mut runner = Fake {
            case: case.into(),
            calls: vec![],
        };
        let result = msgq_sanitizers_check::check(
            std::path::Path::new("/controlled/rust"),
            std::path::Path::new("/controlled/target"),
            &mut runner,
            40 * 1024u64.pow(3),
        );
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{case}: {result:?}"
        );
        // Python records the failed non-string argv attempt; typed Rust rejects at
        // the same stage after the preceding first binary completed.
        let expected = if case == "nonstring" {
            json!(&row["calls"].as_array().unwrap()[..3])
        } else {
            row["calls"].clone()
        };
        assert_eq!(json!(runner.calls), expected, "{case}");
    }
}
#[test]
fn disk_guard_precedes_cargo_and_does_not_create_target() {
    let mut runner = Fake {
        case: "success".into(),
        calls: vec![],
    };
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("missing");
    assert!(
        msgq_sanitizers_check::check(tmp.path(), &target, &mut runner, 4 * 1024u64.pow(3) - 1)
            .is_err()
    );
    assert!(runner.calls.is_empty());
    assert!(!target.exists());
}
#[test]
fn cli_preserves_raw_inherited_streams_and_stops_after_failed_test() {
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let executable = |name: &str, body: String| {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let mut records = vec![];
    for name in ["vision", "creation", "transport", "queued"] {
        let path=executable(name,format!("#!/bin/sh\nprintf '%s\\n' '{name}' >> \"$CALLS\"\nprintf '\\377'\nprintf '\\376' >&2\nif [ '{name}' = vision ]; then IFS= read -r line; [ \"$line\" = 'inherited input' ] || exit 45; fi\n[ \"$CXXFLAGS\" = '-fsanitize=address,undefined -fno-omit-frame-pointer' ] || exit 40\n[ \"$CFLAGS\" = preserved ] || exit 41\n[ \"$ASAN_OPTIONS\" = detect_leaks=1 ] || exit 42\n[ \"$UBSAN_OPTIONS\" = halt_on_error=1 ] || exit 43\n[ -z \"$LD_PRELOAD\" ] || exit 44\nif [ \"$FAIL_SECOND\" = yes ] && [ '{name}' = creation ]; then exit 8; fi\n"));
        records.push(json!({"reason":"compiler-artifact","executable":path}).to_string());
    }
    let rows = records.join("\r\n") + "\r";
    executable(
        "cargo",
        format!("#!/bin/sh\nprintf '\\375' >&2\nprintf '%s' '{rows}'\n"),
    );
    executable(
        "g++",
        "#!/bin/sh\nprintf '\\374' >&2\nprintf ' \\t\\r\\n'\n".into(),
    );
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/msgq-sanitizers/streams.json")).unwrap();
    for expected in oracle.as_array().unwrap() {
        let failed = expected["failed_second"].as_bool().unwrap();
        let calls = dir.join(if failed { "failed-calls" } else { "calls" });
        let mut child = Command::new(env!("CARGO_BIN_EXE_check-msgq-sanitizers"))
            .args(["--target-dir", dir.join("target").to_str().unwrap()])
            .env("PATH", dir)
            .env("CALLS", &calls)
            .env("CFLAGS", "preserved")
            .env("FAIL_SECOND", if failed { "yes" } else { "no" })
            .env_remove("LD_PRELOAD")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"inherited input\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.success(), !failed, "{:?}", output);
        let count = if failed { 2 } else { 4 };
        assert_eq!(output.stdout, vec![255; count]);
        assert_eq!(
            output
                .stdout
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            expected["stdout_hex"].as_str().unwrap()
        );
        assert!(output.stderr.starts_with(&[253, 252]));
        assert_eq!(output.stderr.iter().filter(|&&b| b == 254).count(), count);
        let actual = fs::read_to_string(calls).unwrap();
        assert_eq!(json!(actual.lines().collect::<Vec<_>>()), expected["calls"]);
    }
}
