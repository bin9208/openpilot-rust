#![cfg(feature = "kalman-corpus-generator-native")]

use openpilot_dev_tools::kalman_corpus_generator::{
    functions, generate, input_rows, parse_options, success_line, Model, Options,
};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
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

#[test]
fn output_symlink_is_rejected_before_loading_provider() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("foreign.json");
    let output = directory.path().join("model.json");
    fs::write(&target, b"foreign output").unwrap();
    symlink(&target, &output).unwrap();
    let options = Options {
        oracle: directory.path().join("missing-oracle"),
        output: output.clone(),
    };
    let error = generate(Model::Pose, &options).unwrap_err();
    assert!(error.to_string().contains("output symlink is unsupported"));
    assert_eq!(fs::read_link(&output).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), b"foreign output");
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

fn native_cli(binary: &Path, oracle: &Path, output: &Path, evidence: &Path) {
    let argv = vec![
        binary.as_os_str().to_owned(),
        "--oracle".into(),
        oracle.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
    ];
    let mut command = CommandSpec::new_os(argv.clone(), Duration::from_secs(120));
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
#[ignore = "hosted fresh unchanged Python corpus and original C++ oracle required"]
fn native_cli_matches_fresh_original_complete_json_bits_and_order() {
    let original = required_path("KALMAN_CORPUS_ORIGINAL_ROOT");
    let evidence_root = required_path("KALMAN_CORPUS_EVIDENCE");
    fs::create_dir_all(&evidence_root).unwrap();
    let evidence = evidence_root.join("native-corpus-pair");
    fs::create_dir(&evidence).unwrap();
    for (name, binary_env, oracle_env, count) in [
        ("pose", "KALMAN_POSE_BINARY", "KALMAN_POSE_ORACLE", 16),
        ("car", "KALMAN_CAR_BINARY", "KALMAN_CAR_ORACLE", 24),
    ] {
        let directory = evidence.join(name);
        fs::create_dir(&directory).unwrap();
        let output = directory.join("native.json");
        let source_path = original.join(format!("{name}-original.json"));
        let source_bytes = fs::read(&source_path).unwrap();
        native_cli(
            &required_path(binary_env),
            &required_path(oracle_env),
            &output,
            &directory,
        );
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        let native_bytes = fs::read(&output).unwrap();
        let source = JsonValue::parse(std::str::from_utf8(&source_bytes).unwrap()).unwrap();
        let candidate = JsonValue::parse(std::str::from_utf8(&native_bytes).unwrap()).unwrap();
        let JsonView::Array(rows) = source.view() else {
            panic!("original {name} corpus is not an array");
        };
        assert_eq!(rows.len(), count);
        compare_json(name, &source, &candidate);
        fs::write(
            directory.join("raw-byte-equality"),
            format!("{}\n", source_bytes == native_bytes),
        )
        .unwrap();
    }
}
