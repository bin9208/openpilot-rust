use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_updater_handshake,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
struct Fake {
    case: Value,
    trace: PathBuf,
    calls: Vec<Value>,
}
impl Runner for Fake {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert!(spec.inherit_output);
        assert!(!spec.merge_output);
        assert!(spec.cwd.is_none());
        assert!(spec.env.is_empty());
        self.calls
            .push(json!({"argv":spec.argv,"check":true,"timeout":spec.timeout.as_secs()}));
        match self.case["case"].as_str().unwrap() {
            "strace-timeout" => return Err("controlled timeout".into()),
            "strace-failed" => {
                return Ok(Captured {
                    code: 7,
                    stdout: vec![],
                    stderr: vec![],
                })
            }
            "missing-trace" => {}
            _ => fs::write(&self.trace, self.case["trace"].as_str().unwrap())?,
        }
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn original_thirty_two_receipt_and_trace_cases_preserve_admission_and_exact_invocation() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/ui-updater-handshake/original.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let evidence = temp.path().join("ui-native");
        fs::create_dir(&evidence).unwrap();
        fs::write(
            evidence.join("unit-tests.jsonl"),
            case["receipt"].as_str().unwrap(),
        )
        .unwrap();
        let mut runner = Fake {
            case: case.clone(),
            trace: evidence.join("updater-handshake.trace"),
            calls: vec![],
        };
        let result = ui_updater_handshake::check(&evidence, &mut runner);
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}: {result:?}",
            case["case"]
        );
        let calls: Value = serde_json::from_str(
            &serde_json::to_string(&runner.calls)
                .unwrap()
                .replace(temp.path().to_str().unwrap(), "$ROOT"),
        )
        .unwrap();
        assert_eq!(calls, case["calls"], "{}", case["case"]);
        assert_eq!(
            runner.trace.exists(),
            case["trace_exists"].as_bool().unwrap()
        );
    }
}
#[test]
fn real_cli_preserves_inherited_strace_output_and_failure_exit() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let original: Value =
        serde_json::from_str(include_str!("fixtures/ui-updater-handshake/original.json")).unwrap();
    let success = &original["cases"][0];
    for failed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let evidence = temp.path().join("ui-native");
        fs::create_dir(&evidence).unwrap();
        fs::write(
            evidence.join("unit-tests.jsonl"),
            success["receipt"].as_str().unwrap(),
        )
        .unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let program = bin.join("strace");
        fs::write(&program, "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n if [ \"$1\" = -o ]; then shift; trace=$1; fi\n shift\ndone\nprintf 'child stdout\\n'\nprintf 'child stderr\\n' >&2\nif [ \"$FAIL_TRACE\" = 1 ]; then exit 7; fi\nprintf %s \"$ORIGINAL_TRACE\" > \"$trace\"\n").unwrap();
        fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_check-ui-updater-handshake"))
            .args(["--evidence", evidence.to_str().unwrap()])
            .env("PATH", &bin)
            .env("FAIL_TRACE", if failed { "1" } else { "0" })
            .env("ORIGINAL_TRACE", success["trace"].as_str().unwrap())
            .output()
            .unwrap();
        assert_eq!(output.status.success(), !failed);
        assert_eq!(output.stdout, b"child stdout\n");
        assert!(output.stderr.starts_with(b"child stderr\n"));
        assert_eq!(evidence.join("updater-handshake.trace").exists(), !failed);
    }
    for args in [vec![], vec!["--evidence"], vec!["--unknown", "/tmp"]] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_check-ui-updater-handshake"))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
