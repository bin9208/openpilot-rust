//! Expectations captured from the unchanged CPython checker before native code.
use openpilot_dev_tools::{
    camera_sensor_check::{
        check, compare, general17, generate_requests, parse_lines, split_runner, Random,
    },
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-sensor-check")
                .join(name),
        )
        .unwrap(),
    )
    .unwrap()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn seeded_requests_are_dynamic_and_match_every_original_live_batch() {
    let live = fixture("live-original.json");
    let requests =
        generate_requests(&parse_lines(live["initial_config_stdout"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(requests.len(), 535020);
    let mut all = Sha256::new();
    for (batch, expected) in requests
        .chunks(2000)
        .zip(live["source_calls"].as_array().unwrap())
    {
        let input = batch.join("\n") + "\n";
        assert_eq!(sha(input.as_bytes()), expected["input_sha256"]);
        assert_eq!(batch.len(), expected["lines"].as_u64().unwrap() as usize);
        all.update(input.as_bytes());
    }
    assert_eq!(
        format!("{:x}", all.finalize()),
        live["report"]["input_sha256"]
    );
    for case in fixture("requests-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let result =
            generate_requests(&parse_lines(case["config_stdout"].as_str().unwrap()).unwrap());
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}",
            case["name"]
        );
        if let Ok(requests) = result {
            let input = requests.join("\n") + "\n";
            assert_eq!(requests.len(), case["requests"].as_u64().unwrap() as usize);
            assert_eq!(sha(input.as_bytes()), case["sha256"], "{}", case["name"]);
        }
    }
}
#[test]
fn cpython_random_bit_consumption_general_format_and_shlex_are_independent() {
    let original = fixture("random-original.json");
    let mut rng = Random::new(187);
    for value in original["randoms"].as_array().unwrap() {
        assert_eq!(rng.random().to_bits(), value.as_f64().unwrap().to_bits());
    }
    let mut rng = Random::new(187);
    for (k, value) in [0, 1, 2, 4, 8, 16, 31, 32, 33, 64]
        .into_iter()
        .cycle()
        .zip(original["bits"].as_array().unwrap())
    {
        assert_eq!(rng.getrandbits(k), value.as_u64().unwrap());
    }
    let mut rng = Random::new(187);
    for (n, value) in [1, 2, 3, 4, 7, 16, 31, 32, 33, 100, 1000]
        .into_iter()
        .cycle()
        .zip(original["ranges"].as_array().unwrap())
    {
        assert_eq!(rng.below(n), value.as_u64().unwrap());
    }
    let initial = [
        0.,
        -0.,
        1.,
        1e-4,
        1e-5,
        1e16,
        1e17,
        1.2345678901234567,
        f64::from_bits(1),
        f64::MAX,
    ];
    let values = initial.into_iter().chain(
        original["randoms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap()),
    );
    for (v, expected) in values.zip(original["formatted"].as_array().unwrap()) {
        assert_eq!(general17(v), expected.as_str().unwrap());
    }
    for case in fixture("runner-original.json")["cases"].as_array().unwrap() {
        let result = split_runner(case["input"].as_str().unwrap());
        if case["error"].is_null() {
            assert_eq!(
                result.unwrap(),
                case["result"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(result.is_err());
        }
    }
}
#[test]
fn original_structural_comparison_preserves_f32_bits_python_equality_and_error_repr() {
    for case in fixture("comparison-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let a = JsonValue::parse(case["source"].as_str().unwrap()).unwrap();
        let b = JsonValue::parse(case["native"].as_str().unwrap()).unwrap();
        let result = compare(&a, &b, "probe");
        assert_eq!(result.is_err(), !case["error"].is_null(), "{case}");
        if let Err(error) = result {
            assert_eq!(
                error.message_json(),
                JsonValue::parse(case["error"]["message_json"].as_str().unwrap())
                    .unwrap()
                    .to_json()
                    .unwrap()
            );
            assert_eq!(
                error.is_assertion(),
                case["error"]["type"] == "AssertionError"
            );
        }
    }
}
struct Peer<'a> {
    case: &'a serde_json::Value,
    calls: usize,
    root: &'a Path,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.merge_output && !spec.inherit_output && !spec.inherit_stdin);
        assert_eq!(
            spec.input.as_deref(),
            Some(expected["input"].as_str().unwrap().as_bytes())
        );
        let actual = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_string_lossy()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(actual), expected["argv"]);
        self.calls += 1;
        let (text, code) = match self.calls {
            1 => ("", 0),
            2 => (
                self.case["source_stdout"].as_str().unwrap(),
                self.case["source_code"].as_i64().unwrap() as i32,
            ),
            _ => (
                self.case["native_stdout"].as_str().unwrap(),
                self.case["native_code"].as_i64().unwrap() as i32,
            ),
        };
        Ok(Captured {
            code,
            stdout: text.as_bytes().to_vec(),
            stderr: vec![],
        })
    }
}
#[test]
fn original_process_errors_cardinality_and_artifact_overwrite_order_are_preserved() {
    for case in fixture("process-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let reference = root.join("reference");
        let binary = root.join("native");
        let output = root.join("out");
        fs::write(&reference, b"reference bytes").unwrap();
        fs::write(&binary, b"native bytes").unwrap();
        if case["existing"] == true {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("failure.json"), "old failure\n").unwrap();
            fs::write(output.join("report.json"), "old report\n").unwrap();
        }
        let mut peer = Peer {
            case,
            calls: 0,
            root,
        };
        let mut printed = vec![];
        let result = check(
            &reference,
            &binary,
            &output,
            "",
            "",
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{}",
            case["name"]
        );
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let mut actual = serde_json::Map::new();
        for entry in fs::read_dir(&output).unwrap() {
            let path = entry.unwrap().path();
            actual.insert(
                path.file_name().unwrap().to_str().unwrap().into(),
                serde_json::Value::String(
                    fs::read_to_string(path)
                        .unwrap()
                        .replace(root.to_str().unwrap(), "$ROOT"),
                ),
            );
        }
        assert_eq!(
            serde_json::Value::Object(actual),
            case["files"],
            "{}",
            case["name"]
        );
    }
}
#[test]
fn native_cli_runs_actual_owned_children_without_python_and_classifies_arguments() {
    use std::os::unix::fs::PermissionsExt;
    let bin = env!("CARGO_BIN_EXE_check-camerad-sensors");
    for args in [vec![], vec!["--unknown"], vec!["--reference"]] {
        assert_eq!(
            std::process::Command::new(bin)
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
    assert!(std::process::Command::new(bin)
        .arg("--help")
        .output()
        .unwrap()
        .status
        .success());
    for mismatch in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let reference = root.join("reference");
        let native = root.join("native");
        let marker = root.join("called");
        fs::write(&reference,format!("#!/bin/sh\nif [ ! -e '{}' ]; then : > '{}'; exit 0; fi\nwhile IFS= read -r line; do printf '1.0\\n'; done\n",marker.display(),marker.display())).unwrap();
        fs::write(
            &native,
            format!(
                "#!/bin/sh\nwhile IFS= read -r line; do printf '{}\\n'; done\n",
                if mismatch { "2.0" } else { "1" }
            ),
        )
        .unwrap();
        for path in [&reference, &native] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let result = std::process::Command::new(bin)
            .arg("--reference")
            .arg(&reference)
            .arg("--binary")
            .arg(&native)
            .arg("--output")
            .arg(root.join("out"))
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(result.status.success(), !mismatch, "{:?}", result.stderr);
        assert_eq!(root.join("out/report.json").exists(), !mismatch);
        assert_eq!(root.join("out/failure.json").exists(), mismatch);
    }
}

struct BoundaryPeer<'a> {
    case: &'a serde_json::Value,
    root: &'a Path,
    calls: usize,
}
impl Runner for BoundaryPeer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        assert_eq!(
            spec.input.as_deref(),
            Some(expected["input"].as_str().unwrap().as_bytes())
        );
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_string_lossy()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(argv), expected["argv"]);
        self.calls += 1;
        let mode = self.case["name"].as_str().unwrap();
        if mode == "source-spawn" {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no reference").into());
        }
        if self.calls == 3 {
            match mode {
                "missing-reference-hash" => fs::remove_file(self.root.join("reference"))?,
                "missing-native-hash" => fs::remove_file(self.root.join("native"))?,
                _ => {}
            }
        }
        let mut stdout = if self.calls == 1 {
            vec![]
        } else if mode == "failure-directory" && self.calls == 3 {
            b"2.0\n".repeat(3)
        } else {
            b"1.0\n".repeat(3)
        };
        let mut stderr = vec![];
        if (mode == "source-invalid-utf8" && self.calls == 2)
            || (mode == "native-invalid-utf8" && self.calls == 3)
        {
            stdout = vec![255];
        }
        if mode == "source-stderr-invalid-utf8" && self.calls == 2 {
            stderr = vec![255];
        }
        Ok(Captured {
            code: 0,
            stdout,
            stderr,
        })
    }
}
#[test]
fn original_runner_encoding_spawn_hash_and_output_write_failure_phases_are_preserved() {
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let mode = case["name"].as_str().unwrap();
        if [
            "help",
            "empty",
            "unknown",
            "missing-value",
            "missing-reference",
        ]
        .contains(&mode)
        {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let reference = root.join("reference");
        let binary = root.join("native");
        let output = root.join("out");
        fs::write(&reference, b"reference bytes").unwrap();
        fs::write(&binary, b"native bytes").unwrap();
        if mode == "output-file" {
            fs::write(&output, "existing file\n").unwrap();
        }
        if ["report-directory", "failure-directory"].contains(&mode) {
            fs::create_dir(&output).unwrap();
            fs::create_dir(output.join(if mode == "report-directory" {
                "report.json"
            } else {
                "failure.json"
            }))
            .unwrap();
        }
        let source_runner = match mode {
            "bad-source-quote" => "'broken",
            "runner-words" => "one \"two three\" \"\"",
            _ => "",
        };
        let native_runner = match mode {
            "bad-native-quote" => "\"broken",
            "runner-words" => "four 'five six'",
            _ => "",
        };
        let mut peer = BoundaryPeer {
            case,
            root,
            calls: 0,
        };
        let mut printed = vec![];
        let result = check(
            &reference,
            &binary,
            &output,
            source_runner,
            native_runner,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{mode}: {result:?}"
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{mode}"
        );
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"].as_str().unwrap(),
            "{mode}"
        );
        assert_eq!(
            if output.is_dir() {
                "directory"
            } else if output.exists() {
                "file"
            } else {
                "missing"
            },
            case["output_kind"],
            "{mode}"
        );
        if output.is_dir() {
            let actual = fs::read_dir(&output)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    (
                        path.file_name().unwrap().to_str().unwrap().to_owned(),
                        serde_json::Value::String(if path.is_dir() {
                            "directory".into()
                        } else {
                            fs::read_to_string(&path)
                                .unwrap()
                                .replace(root.to_str().unwrap(), "$ROOT")
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            assert_eq!(serde_json::Value::Object(actual), case["files"], "{mode}");
        }
    }
}

#[test]
fn captured_main_recursion_decoder_and_failure_serializer_boundaries_are_distinct() {
    struct DeepPeer {
        calls: usize,
        depth: usize,
        mismatch: bool,
    }
    impl Runner for DeepPeer {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            self.calls += 1;
            let text = if self.calls == 1 {
                String::new()
            } else {
                let leaf = if self.calls == 3 && self.mismatch {
                    "2"
                } else {
                    "1"
                };
                ("[".repeat(self.depth) + leaf + &"]".repeat(self.depth) + "\n").repeat(3)
            };
            Ok(Captured {
                code: 0,
                stdout: text.into_bytes(),
                stderr: vec![],
            })
        }
    }
    for name in ["main-depth-original.json", "parser-depth-original.json"] {
        for case in fixture(name)["cases"].as_array().unwrap() {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            let source = root.join("source");
            let native = root.join("native");
            let output = root.join("out");
            fs::write(&source, b"source").unwrap();
            fs::write(&native, b"native").unwrap();
            let mut peer = DeepPeer {
                calls: 0,
                depth: case["depth"].as_u64().unwrap() as usize,
                mismatch: case["mismatch"].as_bool().unwrap(),
            };
            let result = check(&source, &native, &output, "", "", &mut peer, &mut vec![]);
            assert_eq!(result.is_err(), case["exit"] != 0, "{name}: {case}");
            let expected_calls = if case["trace"]
                .as_str()
                .unwrap_or("")
                .contains("while decoding a JSON array")
            {
                2
            } else {
                3
            };
            assert_eq!(peer.calls, expected_calls, "{name}: {case}");
            let mut files = fs::read_dir(output)
                .unwrap()
                .map(|p| p.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            files.sort();
            assert_eq!(serde_json::json!(files), case["files"], "{name}: {case}");
        }
    }
}
#[test]
fn original_raw_runner_bytes_symlinks_and_trailing_path_normalization_are_preserved() {
    use std::{
        ffi::OsString,
        os::unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{symlink, PermissionsExt},
        },
    };
    for case in fixture("paths-original.json")["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("reference");
        let native = root.join("native");
        let marker = root.join("called");
        fs::write(&source,format!("#!/bin/sh\nif [ ! -e {} ]; then : > {}; exit 0; fi\nwhile IFS= read -r line; do printf 'null\\n'; done\n",marker.display(),marker.display())).unwrap();
        fs::write(
            &native,
            "#!/bin/sh\nwhile IFS= read -r line; do printf 'null\\n'; done\n",
        )
        .unwrap();
        let raw = root.join(OsString::from_vec(b"runner-\xff".to_vec()));
        fs::write(&raw, "#!/bin/sh\nexec \"$@\"\n").unwrap();
        for path in [&source, &native, &raw] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mode = case["name"].as_str().unwrap();
        let mut source_arg = source.clone().into_os_string();
        if mode == "trailing-reference-path" {
            source_arg.push("/");
        }
        if mode == "symlink-reference" {
            symlink(&source, root.join("link")).unwrap();
            source_arg = root.join("link").into_os_string();
        }
        let mut cli = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-sensors"));
        cli.arg("--reference")
            .arg(source_arg)
            .arg("--binary")
            .arg(&native)
            .arg("--output")
            .arg(root.join("out"))
            .env("PATH", "/nonexistent");
        if mode.starts_with("raw-") {
            let mut word = b"'".to_vec();
            word.extend(raw.as_os_str().as_bytes());
            word.push(b'\'');
            cli.arg(if mode == "raw-reference-runner" {
                "--reference-runner"
            } else {
                "--runner"
            })
            .arg(OsString::from_vec(word));
        }
        let result = cli.output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{mode}: {:?}",
            result.stderr
        );
        let original_report =
            JsonValue::parse(case["files"]["report.json"].as_str().unwrap()).unwrap();
        let normalize = |text: &str| {
            text.replace(
                &original_report
                    .get("source_sha256")
                    .unwrap()
                    .to_utf8()
                    .unwrap(),
                &sha(&fs::read(&source).unwrap()),
            )
            .replace(
                &original_report
                    .get("native_sha256")
                    .unwrap()
                    .to_utf8()
                    .unwrap(),
                &sha(&fs::read(&native).unwrap()),
            )
        };
        assert_eq!(
            String::from_utf8(result.stdout)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            normalize(case["stdout"].as_str().unwrap()),
            "{mode}"
        );
        assert_eq!(
            fs::read_to_string(root.join("out/report.json"))
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            normalize(case["files"]["report.json"].as_str().unwrap()),
            "{mode}"
        );
    }
}
