use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    stats_clock_check::check,
};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn corpus() -> Value {
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/stats-clock/corpus.json.gz").as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
struct Peer {
    corpus: Value,
    mode: String,
    calls: usize,
    integer: Option<String>,
}
impl Peer {
    fn new(mode: &str) -> Self {
        Self {
            corpus: corpus(),
            mode: mode.into(),
            calls: 0,
            integer: None,
        }
    }
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let stage = self.calls;
        self.calls += 1;
        assert!(stage < 4, "unexpected fifth invocation");
        let argv = spec.effective_argv();
        assert!(Path::new(&argv[0]).is_absolute());
        assert_eq!(argv.len(), if stage == 1 { 2 } else { 1 });
        if stage == 1 {
            assert_eq!(argv[1], "--monotonic");
        }
        let rows = if stage < 2 {
            self.corpus[if stage == 0 { "calendar" } else { "monotonic" }]
                .as_array()
                .unwrap()
                .clone()
        } else {
            vec![self.corpus["boundaries"][stage - 2].clone()]
        };
        let input = rows
            .iter()
            .map(|r| format!("{}\n", r["input"].as_str().unwrap()))
            .collect::<String>();
        assert_eq!(
            spec.input.as_deref(),
            Some(input.as_bytes()),
            "stage {stage}"
        );
        if stage < 2 {
            let side = if stage == 0 { "calendar" } else { "monotonic" };
            let mut values = rows
                .iter()
                .map(|r| r["expected"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            if self.mode == format!("{side}-mutation") || self.mode == "both-mutation" {
                values[0] = (values[0].parse::<i128>().unwrap() + 1).to_string();
            }
            if self.mode == format!("{side}-short") {
                values.pop();
            }
            if self.mode == format!("{side}-extra") {
                values.push(values[0].clone());
            }
            if self.mode == format!("{side}-empty") {
                values.clear();
            }
            if self.mode == format!("{side}-invalid") {
                values[0] = "1__2".into();
            }
            if self.mode == "integer-lexical" {
                let negative = values[0].starts_with('-');
                let digits = values[0]
                    .trim_start_matches('-')
                    .chars()
                    .map(|c| {
                        char::from_u32(0xff10 + c.to_digit(10).unwrap())
                            .unwrap()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("_");
                values[0] = format!("\u{2003}{}{digits}\t", if negative { "-" } else { "+" });
            }
            if stage == 0 {
                if let Some(integer) = &self.integer {
                    values[0] = integer.clone();
                }
            }
            let mut stdout = values.iter().map(|v| format!("{v}\n")).collect::<String>();
            if self.mode == "crlf" {
                stdout = stdout.replace('\n', "\r\n");
            }
            if self.mode == "unicode-lines" {
                stdout = stdout.replace('\n', "\u{2028}");
            }
            Ok(Captured {
                code: if self.mode == format!("{side}-exit") {
                    7
                } else {
                    0
                },
                stdout: if self.mode == format!("{side}-invalid-stdout") {
                    vec![255]
                } else {
                    stdout.into_bytes()
                },
                stderr: if self.mode == format!("{side}-invalid-stderr") {
                    vec![255]
                } else {
                    vec![]
                },
            })
        } else {
            let boundary = stage - 2;
            Ok(Captured {
                code: if self.mode == format!("boundary-{boundary}-accepted") {
                    0
                } else if self.mode == "boundary-signals" {
                    -15
                } else {
                    1
                },
                stdout: if self.mode == format!("boundary-{boundary}-invalid-stdout") {
                    vec![255]
                } else {
                    vec![]
                },
                stderr: if self.mode == format!("boundary-{boundary}-invalid-stderr") {
                    vec![255]
                } else if self.mode == "boundary-signals" {
                    "signal diagnostic 가\n".as_bytes().to_vec()
                } else {
                    b"controlled boundary failure\n".to_vec()
                },
            })
        }
    }
}
#[test]
fn original_cases_and_failure_order_preserve_existing_reports_until_final_write() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/stats-clock/failures.json")).unwrap();
    for row in cases {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("candidate");
        fs::write(&binary, b"owned fixture candidate").unwrap();
        let output = temp.path().join("report.json");
        fs::write(&output, b"keep").unwrap();
        let mut peer = Peer::new(row["mode"].as_str().unwrap());
        let mut printed = vec![];
        let result = check(&root(), &binary, &output, &mut peer, &mut printed);
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["mode"]
        );
        assert_eq!(
            peer.calls,
            row["calls"].as_u64().unwrap() as usize,
            "{}",
            row["mode"]
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["stdout"].as_str().unwrap(),
            "{}",
            row["mode"]
        );
        assert_eq!(
            fs::read_to_string(output).unwrap(),
            row["report"].as_str().unwrap(),
            "{}",
            row["mode"]
        );
    }
}
#[test]
fn python_integer_admission_keeps_unicode_digits_and_rejects_invalid_literals() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/stats-clock/integers.json")).unwrap();
    for row in cases {
        // Newlines recognized by splitlines cause cardinality errors before
        // report creation; this test covers each original token's int grammar.
        if row["input"].as_str().unwrap().contains([
            '\x1c', '\x1d', '\x1e', '\n', '\r', '\u{85}', '\u{2028}', '\u{2029}',
        ]) {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("candidate");
        fs::write(&binary, b"owned fixture candidate").unwrap();
        let output = temp.path().join("report.json");
        fs::write(&output, b"keep").unwrap();
        let mut peer = Peer::new("good");
        peer.integer = Some(row["input"].as_str().unwrap().to_owned());
        let mut printed = vec![];
        let result = check(&root(), &binary, &output, &mut peer, &mut printed);
        assert!(result.is_err()); // Every admitted token differs from first calendar result.
        if row["expected"].is_null() {
            assert_eq!(peer.calls, 1, "{:?}", row["input"]);
            assert_eq!(fs::read(output).unwrap(), b"keep");
            assert!(printed.is_empty());
        } else {
            assert_eq!(peer.calls, 4, "{:?}", row["input"]);
            let report =
                openpilot_log_format::JsonValue::parse(&fs::read_to_string(output).unwrap())
                    .unwrap();
            let openpilot_log_format::JsonView::Array(differences) =
                report.get("differences").unwrap().view()
            else {
                panic!("difference list");
            };
            assert_eq!(
                differences[0].get("native").unwrap().to_json().unwrap(),
                row["expected"].as_str().unwrap()
            );
            assert!(!printed.is_empty());
        }
    }
}
#[test]
fn source_drift_and_missing_output_parent_do_not_clobber_existing_evidence() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("source drift must fail before execution")
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("report.json");
    fs::write(&output, b"keep").unwrap();
    let checker = temp.path().join("rust/tools/check_stats_clock.py");
    fs::create_dir_all(checker.parent().unwrap()).unwrap();
    fs::write(checker, b"changed checker").unwrap();
    assert!(check(
        temp.path(),
        Path::new("candidate"),
        &output,
        &mut Never,
        &mut vec![]
    )
    .is_err());
    assert_eq!(fs::read(output).unwrap(), b"keep");
    let binary = temp.path().join("candidate");
    fs::write(&binary, b"owned fixture candidate").unwrap();
    let missing = temp.path().join("missing/report.json");
    let mut peer = Peer::new("good");
    let mut printed = vec![];
    assert!(check(&root(), &binary, &missing, &mut peer, &mut printed).is_err());
    assert_eq!(peer.calls, 4);
    assert!(!missing.parent().unwrap().exists());
    assert!(printed.is_empty());
}
#[test]
fn positional_cli_uses_original_unpacking_status_without_argparse_help() {
    for args in [vec![], vec!["--help"], vec!["one", "two", "three"]] {
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_check-stats-clock"))
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn default_clock_checker_dependency_graph_requires_no_runtime_schema_toolchain() {
    // Follow mandatory local dependencies without compiling them. Adding a
    // helper that pulls cereal/ZMQ into the CI bootstrap must fail this check.
    let mut pending = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(folder) = pending.pop() {
        let folder = folder.canonicalize().unwrap();
        if !visited.insert(folder.clone()) {
            continue;
        }
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(folder.join("Cargo.toml")).unwrap()).unwrap();
        let name = manifest["package"]["name"].as_str().unwrap();
        assert!(
            ![
                "openpilot-carrot-navi",
                "openpilot-logmessaged",
                "openpilot-cereal",
                "openpilot-msgq"
            ]
            .contains(&name),
            "default tool graph imports {name}"
        );
        for (_, dependency) in manifest["dependencies"].as_table().unwrap() {
            if dependency.get("optional").and_then(toml::Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) {
                pending.push(folder.join(path));
            }
        }
    }
}
