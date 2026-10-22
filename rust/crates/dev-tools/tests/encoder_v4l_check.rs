//! Original V4L checks and partial effects, independent of the native implementation.
#![cfg(feature = "camera-sensor-native")]
use openpilot_dev_tools::{
    encoder_v4l_check::{execute, Options},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Read, path::Path};
struct Peer {
    scenario: String,
    commands: Vec<Value>,
}
impl Runner for Peer {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        panic!("V4L uses merged file logs")
    }
    fn run_to_file(&mut self, spec: &CommandSpec, log: &Path) -> Result<Captured> {
        assert_eq!(spec.timeout, std::time::Duration::from_secs(30));
        assert!(spec.disable_core_dump && spec.inherit_stdin);
        let env = spec.effective_env();
        let e = |key: &str| {
            env.get(std::ffi::OsStr::new(key))
                .map(|v| v.to_str().unwrap().to_owned())
        };
        let expected = if spec.argv[0] == "g++" {
            Value::Null
        } else {
            let case = e("ENCODER_FAKE_CASE").unwrap();
            // This peer observes the original captured contract; it does not calculate
            // production expected failures. The runner only emits external child results.
            let original = corpus();
            original["cases"][0]["commands"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| {
                    c["env"]["ENCODER_FAKE_CASE"] == case
                        && c["argv"][0].as_str().unwrap().ends_with(
                            if spec.argv[0].ends_with("rust-probe") {
                                "rust-probe"
                            } else {
                                "source-oracle"
                            },
                        )
                })
                .unwrap()["expected_failure"]
                .clone()
        };
        let mut overlay = BTreeMap::new();
        for key in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "ENCODER_FAKE_CASE",
            "ENCODER_FAKE_TRACE",
            "OPENPILOT_PREFIX",
        ] {
            if let Some(v) = e(key) {
                overlay.insert(key, v);
            }
        }
        self.commands
            .push(json!({"argv":spec.argv,"log":log,"env":overlay,"expected_failure":expected}));
        let mut text = String::from("original controlled child\n");
        let mut code = 0;
        if spec.argv[0] == "g++" {
            let index = if spec.argv.last().unwrap().ends_with("source-oracle") {
                0
            } else {
                1
            };
            if self.scenario == format!("build-failure{index}") {
                code = 9;
            } else {
                fs::write(
                    spec.argv.last().unwrap(),
                    if index == 0 {
                        b"source-oracle".as_slice()
                    } else {
                        b"fake-v4l"
                    },
                )?;
            }
        } else {
            let output = Path::new(&spec.argv[1]);
            fs::create_dir(output)?;
            let rust = spec.argv[0].ends_with("rust-probe");
            if !expected.is_null() {
                code = expected[0].as_i64().unwrap() as i32;
                text.push_str(expected[1].as_str().unwrap());
                text.push('\n');
            }
            if self.scenario
                == if rust {
                    "rust-failure"
                } else {
                    "source-failure"
                }
            {
                code = 7;
            }
            if !expected.is_null() && self.scenario == "wrong-failure-status" {
                code = 0;
            }
            if !expected.is_null() && self.scenario == "wrong-failure-diagnostic" {
                text = "wrong\n".into();
            }
            if self.scenario == "sanitizer" {
                text.push_str("AddressSanitizer: use after free\n");
            }
            // Original wrapper rejects before these mock child artifacts are authored.
            let rejected =
                code != if expected.is_null() {
                    0
                } else {
                    expected[0].as_i64().unwrap() as i32
                } || (!expected.is_null() && !text.contains(expected[1].as_str().unwrap()))
                    || self.scenario == "sanitizer";
            if !rejected {
                let mut lines = [
                    "QUERYCAP msm_vidc_driver",
                    "FORMAT 128 80",
                    "FPS 20",
                    "CROP 128 80",
                    "CONTROL 1 2",
                    "READ_CONTROL 1 2",
                    "BUFFERS 6",
                    "STREAM 1",
                ]
                .map(str::to_owned)
                .to_vec();
                if rust && self.scenario == "config" {
                    lines[0] = "QUERYCAP changed".into();
                }
                for (event, count) in [
                    ("ION_ALLOC", 6),
                    ("ION_FREE", 6),
                    ("CAPTURE_HEADER", 2),
                    ("CAPTURE_FRAME", 24),
                    ("CAPTURE_EOS", 2),
                    ("STOP", 2),
                    ("RETURN_INPUT", 24),
                ] {
                    lines.extend((0..count).map(|i| format!("{event} {i}")));
                }
                lines.extend((0..24).map(|i| format!("QUEUE_INPUT {} {i}", i % 6)));
                if self.scenario == "event-count" {
                    lines.retain(|l| l != "ION_ALLOC 0");
                }
                if self.scenario == "rotation" {
                    let n = lines.len();
                    lines[n - 12] = "QUEUE_INPUT 1 12".into();
                }
                if self.scenario != "driver-missing" {
                    fs::write(e("ENCODER_FAKE_TRACE").unwrap(), lines.join("\n") + "\n")?;
                }
                let mut trace = (0..if self.scenario == "publication-count" {
                    23
                } else {
                    24
                })
                    .map(|i| format!("{i} packet\n"))
                    .collect::<String>();
                if rust && self.scenario == "metadata" {
                    trace = trace.replace("0 packet", "9 packet");
                }
                fs::write(output.join("trace.tsv"), trace)?;
            }
        }
        fs::write(log, text)?;
        Ok(Captured {
            code,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
fn corpus() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/encoder-v4l-check/original.json.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn norm(v: &Value, base: &Path) -> Value {
    match v {
        Value::String(s) => json!(s.replace(base.to_str().unwrap(), "$BASE")),
        Value::Array(a) => Value::Array(a.iter().map(|v| norm(v, base)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.replace(base.to_str().unwrap(), "$BASE"), norm(v, base)))
                .collect(),
        ),
        v => v.clone(),
    }
}
fn files(base: &Path, dir: &Path, out: &mut BTreeMap<String, Value>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files(base, &p, out);
        } else {
            out.insert(
                p.strip_prefix(base.join("out"))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
                norm(&json!({"text":fs::read_to_string(&p).unwrap()}), base),
            );
        }
    }
}
// Original captures remain immutable. Only the withdrawn fixed-floor field
// changes; compare every other artifact byte and command as captured.
fn growth_only_files(original: &Value) -> Value {
    let mut files = original.clone();
    for (name, file) in files.as_object_mut().unwrap() {
        if name == "space.json" || (name.starts_with("build-") && name.ends_with("-space.json")) {
            let text = file["text"].as_str().unwrap();
            let parsed: Value = serde_json::from_str(text).unwrap();
            assert_eq!(parsed["floor"], 25u64 << 30);
            let changed = text.replace("26843545600", "0");
            let expected: Value = serde_json::from_str(&changed).unwrap();
            let mut without_floor = parsed;
            without_floor["floor"] = 0.into();
            assert_eq!(expected, without_floor);
            file["text"] = changed.into();
        }
    }
    files
}

#[test]
fn original_v4l_main_preserves_32_cases_and_all_failure_side_effects() {
    let original = corpus();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for case in original["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let source = base.join("root");
        for name in [
            "rust/tools/encoder_oracle/v4l_adapter.h",
            "rust/tools/encoder_oracle/v4l_base.h",
            "rust/tools/encoder_oracle/v4l_main.cc",
            "rust/tools/encoder_oracle/fake_v4l.cc",
            "msgq_repo/msgq/visionipc/visionbuf.h",
            "msgq_repo/msgq/visionipc/visionbuf_ion.cc",
            "openpilot/system/loggerd/loggerd.h",
            "openpilot/system/loggerd/encoder/encoder.cc",
            "openpilot/system/loggerd/encoder/v4l_encoder.h",
            "openpilot/system/loggerd/encoder/v4l_encoder.cc",
        ] {
            let p = source.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::copy(root.join(name), p).unwrap();
        }
        let name = case["name"].as_str().unwrap();
        let output = base.join("out");
        if name == "existing-output" {
            fs::create_dir(&output).unwrap();
        }
        let binary = base.join("rust-probe");
        fs::write(&binary, b"rust-probe").unwrap();
        let shm = base.join("shm");
        fs::create_dir(&shm).unwrap();
        let opts = Options {
            root: source,
            output: output.clone(),
            binary,
            native: base.join("native"),
            library_path: Some("/inherited libs".into()),
            shm: shm.clone(),
            pid: 12345,
        };
        let mut peer = Peer {
            scenario: name.into(),
            commands: vec![],
        };
        let mut calls = vec![];
        let mut stdout = vec![];
        let result = execute(
            &opts,
            &mut peer,
            &mut |p: &Path| {
                let i = calls.len();
                calls.push(p.to_path_buf());
                Ok(if name == format!("low-space{i}") {
                    1
                } else {
                    100 * 1024 * 1024 * 1024
                })
            },
            &mut stdout,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{name}: {result:?}"
        );
        assert_eq!(
            norm(&json!(peer.commands), base),
            case["commands"],
            "{name} commands"
        );
        assert_eq!(
            norm(&json!(calls), base),
            case["free_calls"],
            "{name} disk calls"
        );
        assert_eq!(
            norm(&json!(String::from_utf8(stdout).unwrap()), base),
            case["stdout"],
            "{name} stdout"
        );
        let mut artifacts = BTreeMap::new();
        files(base, &output, &mut artifacts);
        assert_eq!(
            json!(artifacts),
            growth_only_files(&case["files"]),
            "{name} artifacts"
        );
        assert_eq!(fs::read_dir(shm).unwrap().count(), 0, "{name} cleanup");
    }
}

#[test]
fn original_outcome_requires_status_diagnostic_and_no_sanitizer() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/encoder-v4l-check/outcomes.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let expected = if case["expected"].is_null() {
            None
        } else {
            Some((
                case["expected"][0].as_i64().unwrap() as i32,
                case["expected"][1].as_str().unwrap(),
            ))
        };
        let result = openpilot_dev_tools::encoder_v4l_check::check_outcome(
            case["status"].as_i64().unwrap() as i32,
            case["output"].as_str().unwrap(),
            expected,
        );
        assert_eq!(result.is_err(), !case["error"].is_null(), "{case}");
        if case["error"]
            .as_str()
            .is_some_and(|s| s.contains("sanitizer"))
        {
            assert_eq!(
                result.unwrap_err().to_string(),
                case["error"].as_str().unwrap()
            );
        }
    }
}
