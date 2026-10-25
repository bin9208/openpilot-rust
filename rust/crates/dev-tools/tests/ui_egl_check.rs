use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_egl_check::check,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
const CASES: [&str; 7] = [
    "success",
    "retry",
    "fallback",
    "fail",
    "retry-fail",
    "no-display",
    "init-fail",
];
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn blob(root: &Path, hash: &str) -> Vec<u8> {
    let mut b = vec![];
    flate2::read::GzDecoder::new(
        fs::File::open(root.join("blobs").join(format!("{hash}.gz"))).unwrap(),
    )
    .read_to_end(&mut b)
    .unwrap();
    b
}
struct Peer<'a> {
    row: &'a Value,
    fixtures: PathBuf,
    root: PathBuf,
    output: PathBuf,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = self.calls;
        self.calls += 1;
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(spec.inherit_stdin);
        let mode = self.row["mode"].as_str().unwrap();
        if call == 0 {
            assert_eq!(
                spec.argv,
                vec![
                    String::from("g++"),
                    "-shared".into(),
                    "-fPIC".into(),
                    "-std=c++17".into(),
                    self.root
                        .join("rust/tools/ui_qa/egl_fixture.cc")
                        .to_str()
                        .unwrap()
                        .into(),
                    "-ldl".into(),
                    "-o".into(),
                    self.output.join("libEGL.so").to_str().unwrap().into()
                ]
            );
            assert!(spec.merge_output);
            if mode == "compiler-spawn" {
                return Err("controlled missing compiler".into());
            }
            fs::write(
                self.output.join("libEGL.so"),
                if mode == "compiler-nonzero" {
                    b"partial".as_slice()
                } else {
                    b"private-fixture".as_slice()
                },
            )?;
            return Ok(Captured {
                code: if mode == "compiler-nonzero" { 7 } else { 0 },
                stdout: blob(
                    &self.fixtures,
                    self.row["files"]["build.log"]["blob"].as_str().unwrap(),
                ),
                stderr: vec![],
            });
        }
        let scene = (call - 1) / 2;
        let case = CASES[scene];
        let source = (call - 1).is_multiple_of(2);
        let lane = if source { "source" } else { "native" };
        assert_eq!(
            spec.argv,
            vec![if source { "reference" } else { "candidate" }]
        );
        let env = spec.effective_env();
        assert_eq!(env.get(std::ffi::OsStr::new("EGL_CASE")).unwrap(), case);
        assert_eq!(
            env.get(std::ffi::OsStr::new("LD_LIBRARY_PATH")).unwrap(),
            self.output.as_os_str()
        );
        assert_eq!(
            env.get(std::ffi::OsStr::new("PYTHONPATH")).unwrap(),
            self.root.as_os_str()
        );
        let trace = self.output.join(format!("{case}-{lane}.jsonl"));
        assert_eq!(fs::read(&trace)?, b"");
        let name = format!("{case}-{lane}.jsonl");
        let entry = &self.row["files"][&name];
        if entry.is_object() {
            fs::write(
                &trace,
                blob(&self.fixtures, entry["blob"].as_str().unwrap()),
            )?;
        } else {
            fs::remove_file(&trace)?;
        }
        if !source && case == self.row["target"].as_str().unwrap() {
            return Ok(Captured {
                code: self.row["candidate_status"].as_i64().unwrap() as i32,
                stdout: blob(
                    &self.fixtures,
                    self.row["candidate_stdout_blob"].as_str().unwrap(),
                ),
                stderr: blob(
                    &self.fixtures,
                    self.row["candidate_stderr_blob"].as_str().unwrap(),
                ),
            });
        }
        let stdout = blob(
            &self.fixtures,
            self.row["files"][format!("{case}-{lane}.stdout")]["blob"]
                .as_str()
                .unwrap(),
        );
        let stderr = blob(
            &self.fixtures,
            self.row["files"][format!("{case}-{lane}.stderr")]["blob"]
                .as_str()
                .unwrap(),
        );
        Ok(Captured {
            code: 0,
            stdout,
            stderr: if stderr == b"(no stderr)\n" {
                vec![]
            } else {
                stderr
            },
        })
    }
}
#[test]
fn all_original_checker_cases_preserve_library_admission_streams_results_and_partial_artifacts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-egl");
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-egl/original-checker.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        let mode = row["mode"].as_str().unwrap();
        if mode.starts_with("symlink-") {
            fs::create_dir(&output).unwrap();
            match mode {
                "symlink-file" => fs::write(output.join("libGLESv2.so"), b"preexisting").unwrap(),
                "symlink-existing" => {
                    std::os::unix::fs::symlink("libEGL.so", output.join("libGLESv2.so")).unwrap()
                }
                _ => std::os::unix::fs::symlink("absent", output.join("libGLESv2.so")).unwrap(),
            }
        }
        if mode == "output-file" {
            fs::write(&output, b"file").unwrap();
        }
        let mut peer = Peer {
            row,
            fixtures: fixtures.clone(),
            root: root.clone(),
            output: output.clone(),
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &root,
            Path::new("candidate"),
            Path::new("reference"),
            &output,
            &mut peer,
            &mut printed,
        );
        assert_eq!(result.is_ok(), row["exit"] == 0, "{mode}: {result:?}");
        assert_eq!(
            printed,
            blob(&fixtures, row["stdout_blob"].as_str().unwrap()),
            "{mode}"
        );
        if output.is_dir() {
            let actual = fs::read_dir(&output)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect::<std::collections::BTreeSet<_>>();
            let expected = row["files"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual, expected, "{mode}");
            for (name, entry) in row["files"].as_object().unwrap() {
                let path = output.join(name);
                if let Some(target) = entry["symlink"].as_str() {
                    assert_eq!(fs::read_link(path).unwrap(), Path::new(target));
                } else if name != "libEGL.so" {
                    let b = fs::read(path).unwrap();
                    assert_eq!(
                        b.len() as u64,
                        entry["bytes"].as_u64().unwrap(),
                        "{mode}/{name}"
                    );
                    assert_eq!(sha(&b), entry["sha256"].as_str().unwrap(), "{mode}/{name}");
                }
            }
        }
    }
}
#[test]
fn original_cli_help_errors_are_side_effect_free() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/ui-egl/original-cli.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-ui-egl"))
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{row}"
        );
        assert_eq!(
            !result.stdout.is_empty(),
            row["help"].as_bool().unwrap(),
            "{row}"
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn live_source_and_cffi_dependency_drift_fail_before_compiler() {
    struct Never(usize);
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            self.0 += 1;
            Err("unexpected compiler".into())
        }
    }
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for dependency in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let path = root.join("openpilot/system/ui/lib/egl.py");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(source_root.join("openpilot/system/ui/lib/egl.py"), &path).unwrap();
        fs::copy(source_root.join("uv.lock"), root.join("uv.lock")).unwrap();
        if dependency {
            fs::write(root.join("uv.lock"), "package=[]\n").unwrap();
        } else {
            fs::write(&path, b"changed policy").unwrap();
        }
        let output = temp.path().join("out");
        let mut peer = Never(0);
        let result = check(
            &root,
            Path::new("candidate"),
            Path::new("reference"),
            &output,
            &mut peer,
            &mut vec![],
        );
        assert!(result.is_err());
        assert_eq!(peer.0, 0);
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    }
}
