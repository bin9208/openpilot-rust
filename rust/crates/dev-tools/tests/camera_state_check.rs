//! Independent original camera-state generation, dynamic schema and error phases.
use openpilot_dev_tools::{
    camera_state_check::{check, inputs, Decoded, SchemaDecoder},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn fixture(name: &str) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-state-check")
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
fn original_seed_frame_id_wrap_inputs_exactly_match_all_boundary_hashes() {
    for case in fixture("inputs-original.json")["cases"].as_array().unwrap() {
        let (source, native) = inputs(case["frames"].as_u64().unwrap() as usize).unwrap();
        assert_eq!(sha(source.as_bytes()), case["source_sha256"]);
        assert_eq!(sha(native.as_bytes()), case["native_sha256"]);
        assert_eq!(
            source.len(),
            case["source_bytes"].as_u64().unwrap() as usize
        );
        assert_eq!(
            native.len(),
            case["native_bytes"].as_u64().unwrap() as usize
        );
    }
}
struct Peer<'a> {
    case: &'a serde_json::Value,
    root: &'a Path,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let expected = &self.case["calls"][self.calls];
        self.calls += 1;
        let mode = self.case["name"].as_str().unwrap();
        let lane = if self.calls == 1 { "source" } else { "native" };
        assert_eq!(spec.timeout, Duration::from_secs(180));
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.inherit_stdin && !spec.inherit_output && !spec.merge_output);
        assert_eq!(
            spec.input.as_deref(),
            Some(expected["input"].as_str().unwrap().as_bytes())
        );
        assert_eq!(
            serde_json::json!(spec
                .effective_argv()
                .iter()
                .map(|v| v
                    .to_str()
                    .unwrap()
                    .replace(self.root.to_str().unwrap(), "$ROOT"))
                .collect::<Vec<_>>()),
            expected["argv"]
        );
        if mode == format!("{lane}-spawn") || mode == format!("{lane}-timeout") {
            return Err("owned child process error".into());
        }
        if mode == format!("{lane}-utf8") {
            return Ok(Captured {
                code: 0,
                stdout: vec![255],
                stderr: vec![],
            });
        }
        Ok(Captured {
            code: expected["code"].as_i64().unwrap() as i32,
            stdout: expected["stdout"].as_str().unwrap().as_bytes().to_vec(),
            stderr: expected["stderr"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}
struct SchemaPeer {
    wire: Vec<u8>,
    value: JsonValue,
    loaded: bool,
}
impl SchemaDecoder for SchemaPeer {
    fn load(&mut self, _: &Path) -> Result<()> {
        assert!(!self.loaded);
        self.loaded = true;
        Ok(())
    }
    fn decode(&mut self, wire: &[u8]) -> Result<Decoded> {
        assert!(self.loaded);
        if wire != self.wire {
            return Err("independently captured malformed wire".into());
        }
        Ok(Decoded {
            value: self.value.clone(),
            serialization_error: None,
        })
    }
}
fn assert_process_cases(membership_only: bool) {
    let original = fixture("process-original.json");
    let full_wire = &original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "wire")
        .unwrap();
    let original_fields =
        JsonValue::parse(full_wire["files"]["source.fields.json"].as_str().unwrap()).unwrap();
    let JsonView::Array(fields) = original_fields.view() else {
        panic!()
    };
    let value = fields[0].get("wire").unwrap();
    let wire_json: serde_json::Value = serde_json::from_str(
        full_wire["calls"][0]["stdout"]
            .as_str()
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    let wire = wire_json["wire"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    for case in original["cases"].as_array().unwrap().iter().filter(|case| {
        let name = case["name"].as_str().unwrap();
        let membership = name.starts_with("wire-array-")
            || name.starts_with("wire-text-")
            || name == "wire-null-row";
        membership == membership_only
    }) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let output = root.join("out");
        let source = root.join("source");
        let native = root.join("native");
        fs::write(&source, b"source").unwrap();
        fs::write(&native, b"native").unwrap();
        let mode = case["name"].as_str().unwrap();
        if mode == "missing-source" {
            fs::remove_file(&source).unwrap();
        }
        if mode == "missing-native" {
            fs::remove_file(&native).unwrap();
        }
        if mode == "output-file" {
            fs::write(&output, "old output\n").unwrap();
        }
        if mode.ends_with("-directory") {
            fs::create_dir(&output).unwrap();
            let name = match mode {
                "source-input-directory" => "source.input",
                "source-stdout-directory" => "source.stdout",
                "source-stderr-directory" => "source.stderr",
                "source-fields-directory" => "source.fields.json",
                "runs-directory" => "runs.json",
                "report-directory" => "report.json",
                "failure-directory" => "failure.txt",
                _ => panic!(),
            };
            fs::create_dir(output.join(name)).unwrap();
        }
        if mode == "existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("keep"), "old data\n").unwrap();
        }
        let mut peer = Peer {
            case,
            root,
            calls: 0,
        };
        let mut decoder = SchemaPeer {
            wire: wire.clone(),
            value: value.clone(),
            loaded: false,
        };
        let mut printed = vec![];
        let runner = if mode == "bad-runner" {
            "'"
        } else if mode == "runner" {
            "runner \"two words\" ''"
        } else {
            ""
        };
        let source_runner = if mode == "bad-source-runner" {
            "'"
        } else if mode == "runner" {
            "source-wrapper \\a"
        } else {
            ""
        };
        let result = check(
            &source,
            &native,
            Path::new("$SCHEMA"),
            &output,
            std::ffi::OsStr::new(runner),
            std::ffi::OsStr::new(source_runner),
            1,
            &mut decoder,
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{mode}: {result:?}"
        );
        assert!(decoder.loaded);
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{mode}"
        );
        assert_eq!(
            String::from_utf8(printed)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"],
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
        let files = if output.is_dir() {
            fs::read_dir(&output)
                .unwrap()
                .map(|p| {
                    let p = p.unwrap().path();
                    let value = if p.is_dir() {
                        "directory".into()
                    } else {
                        fs::read_to_string(&p)
                            .unwrap()
                            .replace(root.to_str().unwrap(), "$ROOT")
                    };
                    (
                        p.file_name().unwrap().to_str().unwrap().to_owned(),
                        serde_json::Value::String(value),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
        } else {
            serde_json::Map::new()
        };
        assert_eq!(serde_json::Value::Object(files), case["files"], "{mode}");
    }
}
#[test]
fn original_process_returncode_hash_and_decoding_artifact_order() {
    assert_process_cases(false);
}
#[test]
fn original_wire_membership_failure_precedes_fields_and_native_process() {
    assert_process_cases(true);
}
#[test]
fn original_event_lookup_occurs_only_when_a_wire_row_is_decoded() {
    use std::os::unix::fs::PermissionsExt;
    for case in fixture("root-admission-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let peer = root.join("peer");
        let schema = root.join("root.capnp");
        fs::write(&peer, case["peer"].as_str().unwrap()).unwrap();
        fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&schema, case["schema"].as_str().unwrap()).unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-state"))
            .args([
                "--source",
                peer.to_str().unwrap(),
                "--native",
                peer.to_str().unwrap(),
                "--schema",
                schema.to_str().unwrap(),
                "--output",
                root.join("out").to_str().unwrap(),
                "--frames",
                "1",
            ])
            .output()
            .unwrap();
        assert_eq!(
            result.status.code().unwrap() as i64,
            case["exit"].as_i64().unwrap(),
            "{} wire={}",
            case["name"],
            case["wire"]
        );
        assert_eq!(
            String::from_utf8(result.stdout)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
            case["stdout"]
        );
        let files = fs::read_dir(root.join("out"))
            .unwrap()
            .map(|p| {
                let p = p.unwrap().path();
                (
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    serde_json::Value::String(
                        fs::read_to_string(p)
                            .unwrap()
                            .replace(root.to_str().unwrap(), "$ROOT"),
                    ),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            serde_json::Value::Object(files),
            case["files"],
            "{} wire={}",
            case["name"],
            case["wire"]
        );
    }
}
#[test]
fn original_dynamic_schema_types_unions_defaults_and_data_boundaries() {
    use openpilot_dev_tools::camera_state_check::NativeSchemaDecoder;
    let program = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let schema =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/camera-state-check/shape.capnp");
    let mut decoder = NativeSchemaDecoder::new(PathBuf::from(program));
    decoder.load(&schema).unwrap();
    for case in fixture("schema-original.json")["cases"].as_array().unwrap() {
        let wire = case["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let value = decoder.decode(&wire).unwrap();
        assert_eq!(
            value.serialization_error.as_deref(),
            case["original_error"]["message"].as_str(),
            "{}",
            case["name"]
        );
        if let Some(expected) = case["original_json"].as_str() {
            assert_eq!(
                value.value.to_json().unwrap(),
                JsonValue::parse(expected).unwrap().to_json().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    let pointer_schema = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/camera-state-check/pointers.capnp");
    let mut pointer_decoder = NativeSchemaDecoder::new(PathBuf::from(
        std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into()),
    ));
    pointer_decoder.load(&pointer_schema).unwrap();
    for case in fixture("pointers-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let wire = case["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let actual = pointer_decoder.decode(&wire).unwrap();
        assert_eq!(
            actual.serialization_error.as_deref(),
            case["serialization_error"].as_str(),
            "{}",
            case["name"]
        );
        if let Some(expected) = case["original_json"].as_str() {
            assert_eq!(
                actual.value.to_json().unwrap(),
                JsonValue::parse(expected).unwrap().to_json().unwrap()
            );
        }
    }
    for case in fixture("wire-boundaries-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let wire = case["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let actual = decoder.decode(&wire);
        assert_eq!(
            actual.is_err(),
            !case["error"].is_null(),
            "{}",
            case["name"]
        );
        if let Some(expected) = case["original_json"].as_str() {
            assert_eq!(
                actual.unwrap().value.to_json().unwrap(),
                JsonValue::parse(expected).unwrap().to_json().unwrap()
            );
        }
    }
}
#[test]
fn original_cli_integer_admission_and_partial_output_phases() {
    let temp = tempfile::tempdir().unwrap();
    let schema = temp.path().join("schema.capnp");
    fs::write(
        &schema,
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-state-check/shape.capnp"),
        )
        .unwrap(),
    )
    .unwrap();
    for case in fixture("cli-original.json")["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let run = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-state"))
            .args(case["argv"].as_array().unwrap().iter().map(|v| {
                OsString::from(
                    v.as_str()
                        .unwrap()
                        .replace("$SCHEMA", schema.to_str().unwrap()),
                )
            }))
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert_eq!(
            run.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}"
        );
        assert_eq!(!run.stdout.is_empty(), case["stdout"]);
        assert_eq!(!run.stderr.is_empty(), case["stderr"]);
        assert_eq!(directory.path().join("out").exists(), case["output_exists"]);
        let actual = if directory.path().join("out").is_dir() {
            fs::read_dir(directory.path().join("out"))
                .unwrap()
                .map(|e| {
                    let p = e.unwrap().path();
                    (
                        p.file_name().unwrap().to_str().unwrap().into(),
                        serde_json::Value::String(sha(&fs::read(p).unwrap())),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
        } else {
            serde_json::Map::new()
        };
        assert_eq!(serde_json::Value::Object(actual), case["files"]);
    }
}
