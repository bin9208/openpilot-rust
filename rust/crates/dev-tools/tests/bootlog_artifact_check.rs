//! Independent unchanged checker and uploader-policy captures, never runtime outputs.
use openpilot_dev_tools::bootlog_artifact_check::{
    decode_zstd, next_upload, run_case, scenarios, upload_files,
};
use openpilot_dev_tools::qa_process::NativeRunner;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    process::Command,
};

fn fixture(bytes: &[u8], hash: &str) -> Value {
    assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash);
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn original_generic_journal_exit_syntax_and_raw_output() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/journal-exits-original.json"),
        "16c761eaa7f13354f37c3f903291b9c7307437d2b423c414fa322ed6f89971df",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let result = run_case(
            Path::new("/bin/false"),
            &output,
            &json!({"journal_exit":row["value"]}),
            false,
            &[],
            &mut NativeRunner,
        )
        .unwrap();
        assert_eq!(result["success"], false);
        let command = output.join("commands/journalctl");
        // Library tests execute in libtest; this owned link must use the real
        // registered CLI ELF to exercise production command mode.
        fs::remove_file(&command).unwrap();
        symlink(env!("CARGO_BIN_EXE_check-bootlog-artifacts"), &command).unwrap();
        let actual = Command::new(&command)
            .args(["-o", "short-monotonic"])
            .env("BOOTLOG_ARTIFACT_FIXTURE_ROOT", &output)
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{}",
            row["value"]
        );
        assert_eq!(actual.stdout, unhex(row["stdout_hex"].as_str().unwrap()));
        assert_eq!(
            String::from_utf8(actual.stderr)
                .unwrap()
                .replace(command.to_str().unwrap(), "$JOURNALCTL"),
            row["stderr"],
            "{}",
            row["value"]
        );
    }
}

#[test]
fn all_original_cli_help_and_error_admissions() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/cli-original.json"),
        "5abc269149a5557b71d2a3bedf9242383ee1bbeb696acd63506dd6be0969746f",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let actual = Command::new(env!("CARGO_BIN_EXE_check-bootlog-artifacts"))
            .args(
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code(),
            row["exit"].as_i64().map(|v| v as i32),
            "{}",
            row["args"]
        );
        assert_eq!(
            actual.stdout.is_empty(),
            row["stdout"].as_str().unwrap().is_empty()
        );
        assert_eq!(
            actual.stderr.is_empty(),
            row["stderr"].as_str().unwrap().is_empty()
        );
        if actual.status.success() {
            let help = String::from_utf8(actual.stdout).unwrap();
            for term in [
                "BINARY",
                "ORIGINAL",
                "OUTPUT",
                "--fault-library",
                "--case",
                "--runner",
            ] {
                assert!(help.contains(term), "{help}");
            }
        }
    }
}

#[test]
fn original_helper_delivers_binary_and_unterminated_stdout_before_exec() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/command-output-original.json"),
        "88c4b2b3c1f4cb52131f5ce6a2f1217288fdee2235565218f0c1f5289d42a742",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let command = temp.path().join("journalctl");
        symlink(env!("CARGO_BIN_EXE_check-bootlog-artifacts"), &command).unwrap();
        fs::write(
            temp.path().join("journal.stdout"),
            unhex(row["input_hex"].as_str().unwrap()),
        )
        .unwrap();
        fs::write(temp.path().join("journal.exit"), "0").unwrap();
        let actual = Command::new(command)
            .args(["-o", "short-monotonic"])
            .env("BOOTLOG_ARTIFACT_FIXTURE_ROOT", temp.path())
            .output()
            .unwrap();
        assert_eq!(actual.stdout, unhex(row["stdout_hex"].as_str().unwrap()));
        assert_eq!(actual.stderr, unhex(row["stderr_hex"].as_str().unwrap()));
        assert_eq!(actual.status.code(), row["exit"].as_i64().map(|v| v as i32));
    }
}

#[test]
fn original_retirement_is_narrow_and_every_live_source_pin_is_required() {
    use openpilot_dev_tools::bootlog_artifact_check::{validate_source, SOURCES};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/cases-original.json"),
        "9ae242b100c1570b868430cbca0d11591e2a29566f380b87c949d4f69cc0a433",
    );
    let pins = SOURCES
        .iter()
        .map(|&(name, hash)| (name.to_owned(), json!(hash)))
        .collect::<serde_json::Map<_, _>>();
    assert_eq!(Value::Object(pins), corpus["sources"]);
    let temp = tempfile::tempdir().unwrap();
    for &(name, _) in SOURCES {
        match fs::read(root.join(name)) {
            Ok(bytes) => {
                fs::create_dir_all(temp.path().join(name).parent().unwrap()).unwrap();
                fs::write(temp.path().join(name), bytes).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && [
                        "rust/tools/check_bootlog.py",
                        "rust/tools/check_uploader.py",
                    ]
                    .contains(&name) => {}
            Err(error) => panic!("{name}: {error}"),
        }
    }
    assert!(validate_source(temp.path()).is_ok());
    for &(name, _) in SOURCES {
        let path = temp.path().join(name);
        let original = fs::read(&path).ok();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"modified or reintroduced source\n").unwrap();
        assert!(validate_source(temp.path())
            .unwrap_err()
            .to_string()
            .contains(name));
        if let Some(bytes) = &original {
            fs::write(&path, bytes).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        assert!(validate_source(temp.path()).is_ok());
        if ![
            "rust/tools/check_bootlog.py",
            "rust/tools/check_uploader.py",
        ]
        .contains(&name)
        {
            fs::remove_file(&path).unwrap();
            assert!(validate_source(temp.path())
                .unwrap_err()
                .to_string()
                .contains(name));
            fs::write(&path, original.unwrap()).unwrap();
        }
    }
    let retired = temp.path().join("rust/tools/check_bootlog.py");
    if retired.exists() {
        fs::remove_file(retired).unwrap();
    }
    assert!(validate_source(temp.path()).is_ok());
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|part| u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn all_twenty_original_case_definitions_remain_exact() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/cases-original.json"),
        "9ae242b100c1570b868430cbca0d11591e2a29566f380b87c949d4f69cc0a433",
    );
    let cases = scenarios(Some(Path::new("/synthetic-fault.so")));
    assert_eq!(serde_json::to_value(&cases).unwrap(), corpus["cases"]);
    assert_eq!(scenarios(None).len(), 16);
}

#[test]
fn independent_original_upload_selection_uses_actual_files_and_attributes() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/upload-original.json"),
        "ee521e8c437387938f6eff73df6a398801d14f35024169a0506dd3f9837cee89",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        for file in row["files"].as_array().unwrap() {
            let path = temp.path().join(file[0].as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let kind = file[1].as_str().unwrap();
            match kind {
                "file" => fs::write(&path, b"synthetic\0\xff").unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                _ => symlink(kind.strip_prefix("symlink:").unwrap(), &path).unwrap(),
            }
            if let Some(attr) = file[2].as_str() {
                rustix::fs::setxattr(
                    &path,
                    "user.upload",
                    &unhex(attr),
                    rustix::fs::XattrFlags::empty(),
                )
                .unwrap();
            }
        }
        let metered = row["metered"].as_bool().unwrap();
        let requested = row["requested"].as_str();
        let files = upload_files(temp.path(), metered, requested).unwrap();
        let listing = files
            .iter()
            .map(|file| {
                json!([
                    serde_json::from_str::<Value>(
                        &openpilot_dev_tools::python_fs::json_string(&file.name).unwrap()
                    )
                    .unwrap(),
                    file.key
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(listing), row["listing"], "{}", row["name"]);
        let next = next_upload(temp.path(), metered, requested).unwrap();
        let selected = next.map(|file| {
            assert_eq!(file.path, temp.path().join(&file.key));
            json!([
                serde_json::from_str::<Value>(
                    &openpilot_dev_tools::python_fs::json_string(&file.name).unwrap()
                )
                .unwrap(),
                file.key
            ])
        });
        assert_eq!(json!(selected), row["next"], "{}", row["name"]);
    }
}

#[test]
fn fresh_unchanged_original_artifact_capture_is_bound() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/artifacts-original.json"),
        "045481c27e00475971eccac141bf63384a912161d5bfd4b5e6aab11895594f38",
    );
    assert_eq!(corpus["result"]["result"], "PASS");
    assert_eq!(corpus["cases"].as_object().unwrap().len(), 16);
    let ordinary = &corpus["cases"]["ordinary"];
    assert_eq!(ordinary["success"], true);
    assert_eq!(ordinary["events"].as_array().unwrap().len(), 2);
    assert_eq!(ordinary["events"][1]["boot"]["launchLog"], "launch\0log\n");
}

#[test]
fn actual_original_zstd_full_schema_and_raw_data_are_exact() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/wire-original.json"),
        "943cbca936bf4d91ef5a9fb972e4e815e292286303f8dca012dce188d91b58d9",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let compressed = unhex(row["zstd_hex"].as_str().unwrap());
        let (raw, decoded) = decode_zstd(&compressed).unwrap();
        assert_eq!(raw, unhex(row["capnp_hex"].as_str().unwrap()));
        assert_eq!(decoded, row["decoded"], "{}", row["name"]);
        assert!(decode_zstd(&compressed[..compressed.len() - 1]).is_err());
    }
}

#[test]
fn original_real_child_command_core_limit_and_spawn_failure_phases() {
    let corpus = fixture(
        include_bytes!("fixtures/bootlog-artifacts/process-original.json"),
        "3b617ecc4005560e0df30f10425b6fd57f6a058069dffcbbb9605701aa8f23ee",
    );
    for row in corpus["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let binary = temp.path().join("fixture-child");
        let mode = row["mode"].as_str().unwrap();
        if mode != "missing" {
            fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PARAMS_ROOT/../args\"\nprintf '%s' \"$PWD\" > \"$PARAMS_ROOT/../cwd\"\nulimit -c > \"$PARAMS_ROOT/../core-limit\"\nprintf 'stdout-fixture\\n'\nprintf 'stderr-fixture\\n' >&2\nexit 7\n").unwrap();
            fs::set_permissions(
                &binary,
                fs::Permissions::from_mode(if mode == "nonexec" { 0o644 } else { 0o755 }),
            )
            .unwrap();
        }
        if mode == "exists" {
            fs::create_dir(&output).unwrap();
        }
        let runner = row["runner"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect::<Vec<_>>();
        let actual = run_case(
            &binary,
            &output,
            &json!({}),
            row["native"].as_bool().unwrap(),
            &runner,
            &mut NativeRunner,
        );
        let expected = &row["observed"];
        if let Some(result) = expected.get("result") {
            assert_eq!(actual.unwrap(), *result);
            for key in ["args", "cwd", "core-limit"] {
                assert_eq!(
                    fs::read_to_string(output.join(key))
                        .unwrap()
                        .replace(output.to_str().unwrap(), "$OUTPUT"),
                    expected[key]
                );
            }
            let invocation: Value =
                serde_json::from_slice(&fs::read(output.join("invocation.json")).unwrap()).unwrap();
            assert_eq!(invocation["exit"], 7);
            assert!(
                invocation["start_wall_ns"].as_u64().unwrap()
                    <= invocation["end_wall_ns"].as_u64().unwrap()
            );
            let prefix = format!(
                "boot-fixture-{}",
                &format!("{:x}", Sha256::digest(output.to_str().unwrap().as_bytes()))[..12]
            );
            assert_eq!(invocation["environment"]["OPENPILOT_PREFIX"], prefix);
            let mut normalized = invocation;
            normalized["environment"]["OPENPILOT_PREFIX"] =
                expected["invocation"]["environment"]["OPENPILOT_PREFIX"].clone();
            for field in ["start_wall_ns", "end_wall_ns", "binary_sha256"] {
                normalized.as_object_mut().unwrap().remove(field);
            }
            let text = serde_json::to_string(&normalized)
                .unwrap()
                .replace(output.to_str().unwrap(), "$OUTPUT")
                .replace(binary.to_str().unwrap(), "$BINARY");
            assert_eq!(
                serde_json::from_str::<Value>(&text).unwrap(),
                expected["invocation"]
            );
        } else {
            let error = actual.unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                expected["errno"].as_i64().map(|v| v as i32)
            );
        }
        for file in ["stderr.log", "invocation.json", "result.json"] {
            assert_eq!(
                output.join(file).exists(),
                expected[format!("{file}-exists")].as_bool().unwrap()
            );
        }
        if expected["stderr.log-exists"] == true {
            assert_eq!(
                fs::read_to_string(output.join("stderr.log")).unwrap(),
                expected["stderr.log"]
            );
        }
    }
}
