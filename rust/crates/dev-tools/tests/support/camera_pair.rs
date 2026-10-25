use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn fixture(family: &str, name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/camera-{family}-check/{name}")),
        )
        .unwrap(),
    )
    .unwrap()
}
pub type Check =
    fn(&Path, &Path, &Path, &str, &str, &mut dyn Runner, &mut dyn std::io::Write) -> Result<()>;
struct Peer<'a> {
    case: &'a serde_json::Value,
    root: &'a Path,
    output: &'a Path,
    calls: usize,
    hashes: Vec<(String, String)>,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        let mode = self.case["name"].as_str().unwrap();
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.merge_output && !spec.inherit_output && !spec.inherit_stdin);
        let argv = spec
            .effective_argv()
            .iter()
            .map(|v| {
                v.to_string_lossy()
                    .replace(self.root.to_str().unwrap(), "$ROOT")
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(argv), expected["argv"]);
        let input = String::from_utf8(spec.input.clone().unwrap()).unwrap();
        let canonical = input.replace(self.output.to_str().unwrap(), "$OUTPUT");
        assert_eq!(sha(canonical.as_bytes()), expected["input_sha256"]);
        // Validate exact bytes in the independent original invocation context,
        // then retain the current-path hash for its receipt comparison.
        let original_input = input.replace(
            self.root.to_str().unwrap(),
            self.case["invocation_root"].as_str().unwrap(),
        );
        assert_eq!(
            sha(original_input.as_bytes()),
            expected["raw_input_sha256"],
            "raw invocation input"
        );
        self.hashes.push((
            sha(input.as_bytes()),
            expected["raw_input_sha256"].as_str().unwrap().to_owned(),
        ));
        let len = input.lines().count();
        assert_eq!(len, expected["lines"].as_u64().unwrap() as usize);
        self.calls += 1;
        let first = self.calls == 1;
        if !first && mode.starts_with("hash-") {
            fs::remove_file(self.root.join(if mode == "hash-source-missing" {
                "reference"
            } else {
                "native"
            }))?;
        }
        let mut lines = vec!["null"; len];
        if ["mismatch", "mismatch-existing", "failure-directory"].contains(&mode) {
            lines[1] = if first { "1.0" } else { "2.0" };
        }
        if mode == "key-mismatch" {
            lines[1] = if first { r#"{"a":1}"# } else { r#"{"b":1}"# };
        }
        if mode == "overflow" {
            lines[1] = if first { "1.0" } else { "1e39" };
        }
        if mode == "surrogate" {
            lines[1] = if first { r#""\ud800""# } else { r#""x""# };
        }
        if mode == "source-json" && first || mode == "native-json" && !first {
            lines[1] = "invalid";
        }
        if mode == "source-count" && first {
            lines.pop();
        }
        let separator = if mode == "splitlines" { "\r\n" } else { "\n" };
        let mut stdout = (lines.join(separator) + separator).into_bytes();
        let mut stderr = vec![];
        if mode == "source-utf8" && first || mode == "native-utf8" && !first {
            stdout = vec![255];
        }
        if mode == "source-stderr-utf8" && first {
            stderr = vec![255];
        }
        let code = if mode == "source-exit" && first || mode == "native-exit" && !first {
            7
        } else {
            0
        };
        Ok(Captured {
            code,
            stdout,
            stderr,
        })
    }
}
pub fn process_matrix(family: &str, check: Check) {
    for case in fixture(family, "process-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let mode = case["name"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let reference = root.join("reference");
        let native = root.join("native");
        let output = root.join("out");
        fs::write(&reference, b"reference bytes").unwrap();
        fs::write(&native, b"native bytes").unwrap();
        if mode == "output-file" {
            fs::write(&output, "old output\n").unwrap();
        }
        if [
            "mismatch-existing",
            "report-directory",
            "failure-directory",
            "fixtures-file",
            "last-fixture-directory",
        ]
        .contains(&mode)
        {
            fs::create_dir(&output).unwrap();
            match mode {
                "mismatch-existing" => {
                    fs::write(output.join("report.json"), "old report\n").unwrap();
                    fs::write(output.join("failure.json"), "old failure\n").unwrap();
                }
                "last-fixture-directory" => {
                    fs::create_dir(output.join("fixtures")).unwrap();
                    fs::create_dir(output.join("fixtures/image-182.bin")).unwrap();
                }
                "fixtures-file" => fs::write(output.join("fixtures"), "old fixtures\n").unwrap(),
                _ => fs::create_dir(output.join(if mode == "report-directory" {
                    "report.json"
                } else {
                    "failure.json"
                }))
                .unwrap(),
            }
        }
        let sr = match mode {
            "bad-source-runner" => "'broken",
            "runner-words" => "one \"two three\" \"\"",
            _ => "",
        };
        let nr = match mode {
            "bad-native-runner" => "\"broken",
            "runner-words" => "four 'five six'",
            _ => "",
        };
        let mut peer = Peer {
            case,
            root,
            output: &output,
            calls: 0,
            hashes: vec![],
        };
        let mut printed = vec![];
        let result = check(
            &reference,
            &native,
            &output,
            sr,
            nr,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{family}/{mode}: {result:?}"
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{family}/{mode}"
        );
        let normalize = |text: String| {
            let mut text = text.replace(root.to_str().unwrap(), "$ROOT");
            for (current, original) in &peer.hashes {
                text = text.replace(current, original);
            }
            text
        };
        assert_eq!(
            normalize(String::from_utf8(printed).unwrap()),
            case["stdout"].as_str().unwrap(),
            "{family}/{mode}"
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
            "{family}/{mode}"
        );
        let mut actual = serde_json::Map::new();
        if output.is_dir() {
            for entry in fs::read_dir(&output).unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().unwrap() == "fixtures" {
                    continue;
                }
                actual.insert(
                    path.file_name().unwrap().to_str().unwrap().to_owned(),
                    serde_json::Value::String(if path.is_dir() {
                        "directory".into()
                    } else {
                        normalize(fs::read_to_string(path).unwrap())
                    }),
                );
            }
        }
        assert_eq!(
            serde_json::Value::Object(actual),
            case["files"],
            "{family}/{mode}"
        );
        let fixtures = output.join("fixtures");
        if let Some(expected) = case["fixtures"].as_object() {
            let mut actual = serde_json::Map::new();
            for entry in fs::read_dir(&fixtures).unwrap() {
                let path = entry.unwrap().path();
                actual.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    if path.is_dir() {
                        serde_json::json!("directory")
                    } else {
                        let bytes = fs::read(path).unwrap();
                        serde_json::json!({"bytes":bytes.len(),"sha256":sha(&bytes)})
                    },
                );
            }
            assert_eq!(&actual, expected, "{family}/{mode}");
        } else {
            assert!(!fixtures.is_dir(), "{family}/{mode}");
            if mode == "fixtures-file" {
                assert_eq!(fs::read_to_string(fixtures).unwrap(), "old fixtures\n");
            }
        }
    }
}
pub fn cli_arguments(bin: &str) {
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
}
pub fn depth_matrix(family: &str, check: Check) {
    struct Peer {
        calls: usize,
        depth: usize,
        mismatch: bool,
    }
    impl Runner for Peer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.calls += 1;
            let count = String::from_utf8(spec.input.clone().unwrap())
                .unwrap()
                .lines()
                .count();
            let leaf = if self.calls == 2 && self.mismatch {
                "2"
            } else {
                "1"
            };
            let first = "[".repeat(self.depth) + leaf + &"]".repeat(self.depth) + "\n";
            let stdout = first + "null\n".repeat(count - 1).as_str();
            Ok(Captured {
                code: 0,
                stdout: stdout.into_bytes(),
                stderr: vec![],
            })
        }
    }
    for case in fixture(family, "depth-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let reference = root.join("reference");
        let native = root.join("native");
        let output = root.join("out");
        fs::write(&reference, "reference").unwrap();
        fs::write(&native, "native").unwrap();
        let mut peer = Peer {
            calls: 0,
            depth: case["depth"].as_u64().unwrap() as usize,
            mismatch: case["mismatch"].as_bool().unwrap(),
        };
        let result = check(&reference, &native, &output, "", "", &mut peer, &mut vec![]);
        assert_eq!(result.is_err(), case["exit"] != 0, "{family}: {case}");
        assert_eq!(peer.calls, 2);
        assert_eq!(output.exists(), case["output_created"], "{family}: {case}");
        let mut files = fs::read_dir(output)
            .unwrap()
            .map(|p| p.unwrap().file_name().into_string().unwrap())
            .filter(|name| name != "fixtures")
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(serde_json::json!(files), case["files"], "{family}: {case}");
    }
}
