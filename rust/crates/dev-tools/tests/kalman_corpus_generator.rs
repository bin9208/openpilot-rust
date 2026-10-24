#![cfg(feature = "kalman-corpus-generator-native")]

use openpilot_dev_tools::kalman_corpus_generator::{
    functions, generate, input_rows, parse_options, success_line, Model, Options,
};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};

fn original_rows(model: Model) -> Vec<JsonValue> {
    let source = match model {
        Model::Pose => include_str!("../../locationd/tests/data/model.json"),
        Model::Car => include_str!("../../paramsd/tests/data/model.json"),
    };
    let value = JsonValue::parse(source).unwrap();
    let JsonView::Array(rows) = value.view() else {
        panic!("original model corpus is not an array");
    };
    rows
}

fn bits(values: JsonValue) -> Vec<u64> {
    let JsonView::Array(values) = values.view() else {
        panic!("original input is not an array");
    };
    values
        .iter()
        .map(|value| match value.view() {
            JsonView::Float(value) => value.to_bits(),
            _ => panic!("original input is not a float"),
        })
        .collect()
}

#[test]
fn every_native_draw_matches_independent_original_input_bits() {
    for (model, count, width) in [(Model::Pose, 16, 18), (Model::Car, 24, 9)] {
        let original = original_rows(model);
        let native = input_rows(model);
        assert_eq!(original.len(), count);
        assert_eq!(native.len(), count);
        for (index, (source, row)) in original.iter().zip(&native).enumerate() {
            assert_eq!(row.state.len(), width);
            assert_eq!(
                bits(source.get("state").unwrap()),
                row.state.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "{model:?} state row {index}"
            );
            assert_eq!(
                bits(
                    source
                        .get("globals")
                        .unwrap_or_else(|| JsonValue::parse("[]").unwrap())
                ),
                row.globals
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                "{model:?} globals row {index}"
            );
            let JsonView::Float(dt) = source.get("dt").unwrap().view() else {
                panic!("original dt is not a float");
            };
            assert_eq!(dt.to_bits(), row.dt.to_bits(), "{model:?} dt row {index}");
        }
        if model == Model::Pose {
            assert!(native[0].state.iter().all(|value| value.to_bits() == 0));
        }
    }
}

#[test]
fn function_roster_retains_original_order_and_matrix_widths() {
    let pose = functions(Model::Pose);
    assert_eq!(
        pose.iter()
            .map(|function| function.name)
            .collect::<Vec<_>>(),
        ["f_fun", "F_fun", "h_4", "H_4", "h_10", "H_10", "h_13", "H_13", "h_14", "H_14"]
    );
    assert_eq!(
        pose.iter()
            .map(|function| function.width)
            .collect::<Vec<_>>(),
        [18, 324, 3, 54, 3, 54, 3, 54, 3, 54]
    );
    let car = functions(Model::Car);
    assert_eq!(
        car.iter().map(|function| function.name).collect::<Vec<_>>(),
        [
            "f_fun", "F_fun", "h_24", "H_24", "h_25", "H_25", "h_26", "H_26", "h_27", "H_27",
            "h_28", "H_28", "h_29", "H_29", "h_30", "H_30", "h_31", "H_31",
        ]
    );
    assert_eq!(
        car.iter()
            .map(|function| function.width)
            .collect::<Vec<_>>(),
        [9, 81, 2, 18, 1, 9, 1, 9, 1, 9, 1, 9, 1, 9, 1, 9, 1, 9]
    );
    assert!(pose.iter().take(2).all(|function| function.timed));
    assert!(car.iter().skip(2).all(|function| !function.timed));
}

#[test]
fn argparse_preserves_raw_paths_and_original_required_option() {
    let raw = OsString::from_vec(b"/tmp/corpus-\xff.json".to_vec());
    let options = parse_options(
        [
            OsString::from("--or=older"),
            OsString::from("--oracle"),
            OsString::from("oracle-root"),
            OsString::from("--output"),
            raw.clone(),
        ],
        "unused",
    )
    .unwrap()
    .unwrap();
    assert_eq!(options.oracle, PathBuf::from("oracle-root"));
    assert_eq!(options.output.as_os_str().as_bytes(), raw.as_bytes());
    let mut stdout = b"Generated 16 actual-source model/Jacobian samples: ".to_vec();
    stdout.extend_from_slice(raw.as_bytes());
    stdout.push(b'\n');
    assert_eq!(success_line(Model::Pose, &options.output), stdout);
    assert!(parse_options([OsString::from("--help")], "unused")
        .unwrap()
        .is_none());
    assert!(parse_options([], "unused").is_err());
    assert!(parse_options([OsString::from("--oracle")], "unused").is_err());
    assert!(parse_options([OsString::from("--bogus")], "unused").is_err());
}

#[test]
fn rejected_provider_never_replaces_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("model.json");
    let oracle = directory.path().join("oracle");
    fs::create_dir(&oracle).unwrap();
    fs::write(&output, b"foreign output").unwrap();
    let options = Options {
        oracle: oracle.clone(),
        output: output.clone(),
    };
    assert!(generate(Model::Pose, &options).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"foreign output");
    fs::write(
        oracle.join("pose.h"),
        b"void pose_f_fun(double *x, double dt, double *out);\n",
    )
    .unwrap();
    fs::write(oracle.join("libpose.so"), b"not a shared library").unwrap();
    assert!(generate(Model::Pose, &options).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"foreign output");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

fn compare_json(label: &str, source: &JsonValue, candidate: &JsonValue) {
    match (source.view(), candidate.view()) {
        (JsonView::Array(a), JsonView::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label} array length");
            for (index, (a, b)) in a.iter().zip(&b).enumerate() {
                compare_json(&format!("{label}/{index}"), a, b);
            }
        }
        (JsonView::Object(a), JsonView::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{label} object length");
            for (index, ((key_a, a), (key_b, b))) in a.iter().zip(&b).enumerate() {
                assert_eq!(key_a, key_b, "{label} key order {index}");
                compare_json(&format!("{label}/{index}"), a, b);
            }
        }
        (JsonView::Float(a), JsonView::Float(b)) => {
            assert_eq!(a.to_bits(), b.to_bits(), "{label} float bits");
        }
        _ => panic!("{label} JSON type changed"),
    }
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn assert_retired_sources_absent(root: &Path) {
    assert!(root.is_absolute(), "retired source root must be absolute");
    assert!(
        root.join("rust/tools").is_dir(),
        "retired source parent missing"
    );
    for source in ["generate_pose_corpus.py", "generate_car_corpus.py"] {
        let path = root.join("rust/tools").join(source);
        let error = fs::symlink_metadata(&path).unwrap_err();
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "{}",
            path.display()
        );
    }
}

fn native_cli(binary: &Path, oracle: &Path, output: &Path, evidence: &Path, cwd: &Path) {
    assert!(binary.is_absolute() && oracle.is_absolute() && output.is_absolute());
    assert!(cwd.is_absolute());
    let argv = vec![
        binary.as_os_str().to_owned(),
        "--oracle".into(),
        oracle.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
    ];
    let mut command = CommandSpec::new_os(argv.clone(), Duration::from_secs(120));
    command.cwd = Some(cwd.to_path_buf());
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 1024 * 1024;
    command.env.insert("PATH".into(), "/nonexistent".into());
    command.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    fs::write(
        evidence.join("argv.json"),
        serde_json::to_vec_pretty(&argv.iter().map(|arg| arg.as_bytes()).collect::<Vec<_>>())
            .unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("cwd.json"),
        serde_json::to_vec_pretty(cwd.as_os_str().as_bytes()).unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("env.txt"),
        b"PATH=/nonexistent\nremove PYTHONPATH\nremove PYTHONHOME\n",
    )
    .unwrap();
    let result =
        NativeRunner.run_to_files(&command, &evidence.join("stdout"), &evidence.join("stderr"));
    match result {
        Ok(result) => {
            fs::write(evidence.join("status"), format!("{}\n", result.code)).unwrap();
            assert_eq!(
                result.code,
                0,
                "native CLI failed; see {}",
                evidence.display()
            );
        }
        Err(error) => {
            fs::write(evidence.join("status"), format!("runner error: {error}\n")).unwrap();
            panic!("native CLI failed; see {}: {error}", evidence.display());
        }
    }
}

#[test]
#[ignore = "hosted independently captured original data and fresh C++ oracle required"]
fn native_cli_matches_fresh_original_complete_json_bits_and_order() {
    let original = required_path("KALMAN_CORPUS_ORIGINAL_ROOT");
    let evidence_root = required_path("KALMAN_CORPUS_EVIDENCE");
    let retired_root = required_path("KALMAN_CORPUS_RETIRED_ROOT");
    assert!(original.is_absolute() && evidence_root.is_absolute());
    assert_retired_sources_absent(&retired_root);
    fs::create_dir_all(&evidence_root).unwrap();
    let evidence = evidence_root.join("native-corpus-pair");
    fs::create_dir(&evidence).unwrap();
    let empty_cwd = tempfile::Builder::new()
        .prefix("empty-cwd-")
        .tempdir_in(&evidence)
        .unwrap();
    for (name, binary_env, oracle_env, count, expected_sha) in [
        (
            "pose",
            "KALMAN_POSE_BINARY",
            "KALMAN_POSE_ORACLE",
            16,
            "f1e62ab1548397140f61ede4fd7e0c33d95356c16b99ea6223256ea33c235283",
        ),
        (
            "car",
            "KALMAN_CAR_BINARY",
            "KALMAN_CAR_ORACLE",
            24,
            "f568f3ad3cae09a94abded4eb1b475207d46f40c08965e27fb7272fbfdf0aebf",
        ),
    ] {
        let directory = evidence.join(name);
        fs::create_dir(&directory).unwrap();
        let output = directory.join("native.json");
        let source_path = original.join(format!("{name}-original.json"));
        let source_bytes = fs::read(&source_path).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&source_bytes)), expected_sha);
        fs::write(
            directory.join("original-before.sha256"),
            format!("{expected_sha}\n"),
        )
        .unwrap();
        let binary = required_path(binary_env);
        let oracle = required_path(oracle_env);
        let binary_before = fs::metadata(&binary).unwrap();
        assert!(binary_before.is_file());
        let binary_sha = Sha256::digest(fs::read(&binary).unwrap());
        fs::write(
            directory.join("binary-before.sha256"),
            format!("{binary_sha:x}\n"),
        )
        .unwrap();
        native_cli(&binary, &oracle, &output, &directory, &retired_root);
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        let native_bytes = fs::read(&output).unwrap();
        let source = JsonValue::parse(std::str::from_utf8(&source_bytes).unwrap()).unwrap();
        let candidate = JsonValue::parse(std::str::from_utf8(&native_bytes).unwrap()).unwrap();
        let JsonView::Array(rows) = source.view() else {
            panic!("original {name} corpus is not an array");
        };
        assert_eq!(rows.len(), count);
        compare_json(name, &source, &candidate);
        assert_eq!(native_bytes, source_bytes, "{name} original JSON bytes");
        fs::write(
            directory.join("raw-byte-equality"),
            format!("{}\n", source_bytes == native_bytes),
        )
        .unwrap();
        let isolated = directory.join("isolated");
        fs::create_dir(&isolated).unwrap();
        let isolated_output = isolated.join("native.json");
        native_cli(
            &binary,
            &oracle,
            &isolated_output,
            &isolated,
            empty_cwd.path(),
        );
        assert_eq!(fs::read(&isolated_output).unwrap(), source_bytes);
        assert_eq!(fs::read(&output).unwrap(), source_bytes);
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        assert_retired_sources_absent(&retired_root);
        if name == "pose" {
            for link_kind in ["symlink", "hardlink"] {
                let link_dir = directory.join(link_kind);
                fs::create_dir(&link_dir).unwrap();
                let target = link_dir.join("target.json");
                let linked_output = link_dir.join("output.json");
                fs::write(&target, b"preexisting output").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
                match link_kind {
                    "symlink" => symlink(&target, &linked_output).unwrap(),
                    "hardlink" => fs::hard_link(&target, &linked_output).unwrap(),
                    _ => unreachable!(),
                }
                let before = fs::metadata(&target).unwrap();
                native_cli(
                    &binary,
                    &oracle,
                    &linked_output,
                    &link_dir,
                    empty_cwd.path(),
                );
                let after = fs::metadata(&target).unwrap();
                assert_eq!(after.ino(), before.ino(), "{link_kind} target inode");
                assert_eq!(after.mode(), before.mode(), "{link_kind} target mode");
                if link_kind == "symlink" {
                    assert_eq!(fs::read_link(&linked_output).unwrap(), target);
                } else {
                    assert_eq!(fs::metadata(&linked_output).unwrap().ino(), after.ino());
                }
                let linked_bytes = fs::read(&target).unwrap();
                assert_eq!(linked_bytes, native_bytes, "{link_kind} output bytes");
                let linked = JsonValue::parse(std::str::from_utf8(&linked_bytes).unwrap()).unwrap();
                compare_json(link_kind, &source, &linked);
            }
        }
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        fs::write(
            directory.join("original-after.sha256"),
            format!("{:x}\n", Sha256::digest(fs::read(&source_path).unwrap())),
        )
        .unwrap();
        let binary_after = fs::metadata(&binary).unwrap();
        assert_eq!(binary_after.dev(), binary_before.dev());
        assert_eq!(binary_after.ino(), binary_before.ino());
        assert_eq!(binary_after.len(), binary_before.len());
        let binary_after_sha = Sha256::digest(fs::read(&binary).unwrap());
        fs::write(
            directory.join("binary-after.sha256"),
            format!("{binary_after_sha:x}\n"),
        )
        .unwrap();
        assert_eq!(binary_after_sha, binary_sha);
    }
    assert_eq!(fs::read_dir(empty_cwd.path()).unwrap().count(), 0);
    assert_retired_sources_absent(&retired_root);
}
