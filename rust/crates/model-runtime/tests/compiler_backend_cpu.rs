#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};
fn fixture(name: &str) -> serde_json::Value {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-lowering.json")).unwrap();
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap()
        .clone()
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_broadcast_outputs_survive_native_onnx_compile_artifact_and_runtime() {
    let case = fixture("Add-1");
    let directory = tempfile::tempdir().unwrap();
    let onnx = directory.path().join("model.onnx");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    fs::write(&onnx, &bytes).unwrap();
    let destination = directory.path().join("bundle");
    let compilation = compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions::default(),
        &destination,
    )
    .unwrap();
    assert_eq!(compilation.graph.calls.len(), 1);
    let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
    model.run();
    let mut output = vec![0; model.output_size("output").unwrap()];
    model.read_output("output", &mut output).unwrap();
    assert_eq!(
        output,
        decode(
            case["expected"]
                .as_str()
                .unwrap()
                .split('\t')
                .nth(4)
                .unwrap()
        )
    );
    assert!(fs::read_to_string(destination.join("kernels.c"))
        .unwrap()
        .contains(&compilation.model_sha256));
}
#[test]
fn original_offline_bindings_execute_through_existing_model_run_cli() {
    let case = fixture("offline-bindings-10-symbol");
    let directory = tempfile::tempdir().unwrap();
    let onnx = directory.path().join("model.onnx");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    fs::write(&onnx, &bytes).unwrap();
    let destination = directory.path().join("bundle");
    let options = CompileOptions {
        offline_bindings: true,
        ..CompileOptions::default()
    };
    compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &options,
        &destination,
    )
    .unwrap();
    fs::write(
        directory.path().join("input.bin"),
        decode(case["inputs"][0]["data_hex"].as_str().unwrap()),
    )
    .unwrap();
    fs::write(
        directory.path().join("sequence.json"),
        r#"[{"inputs":{"a":"input.bin"},"outputs":{"output":"output.bin"}}]"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_model-run"))
        .arg("--trusted-bundle")
        .arg(&destination)
        .arg(directory.path().join("sequence.json"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(directory.path().join("output.bin")).unwrap(),
        decode(
            case["expected"]
                .as_str()
                .unwrap()
                .split('\t')
                .nth(4)
                .unwrap()
        )
    );
}

#[test]
fn original_composed_pipeline_survives_planning_and_disabled_planning() {
    let case = fixture("CPU-pipeline-transpose-add-mul");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let expected = decode(
        case["expected"]
            .as_str()
            .unwrap()
            .split('\t')
            .nth(4)
            .unwrap(),
    );
    for memory_planning in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let onnx = directory.path().join("model.onnx");
        fs::write(&onnx, &bytes).unwrap();
        let destination = directory.path().join("bundle");
        let options = CompileOptions {
            memory_planning,
            ..CompileOptions::default()
        };
        let compilation = compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &options,
            &destination,
        )
        .unwrap();
        assert_eq!(compilation.graph.calls.len(), 3);
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        model.run();
        let mut output = vec![0; model.output_size("output").unwrap()];
        model.read_output("output", &mut output).unwrap();
        assert_eq!(output, expected);
    }
}
#[test]
fn identity_unsupported_ir_and_existing_destination_fail_before_compiler_or_publication() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("bundle");
    let onnx = directory.path().join("model.onnx");
    let case = fixture("Add-1");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    fs::write(&onnx, &bytes).unwrap();
    let options = CompileOptions {
        program: directory.path().join("missing-compiler"),
        ..CompileOptions::default()
    };
    assert!(compile_onnx(&onnx, &"0".repeat(64), &options, &destination)
        .unwrap_err()
        .contains("identity"));
    assert!(!destination.exists());
    let case = fixture("Softmax-1-None");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    fs::write(&onnx, &bytes).unwrap();
    assert!(compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &options,
        &destination
    )
    .unwrap_err()
    .contains("not implemented"));
    assert!(!destination.exists());
    fs::write(&destination, b"user-owned").unwrap();
    assert!(compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &options,
        &destination
    )
    .unwrap_err()
    .contains("already exists"));
    assert_eq!(fs::read(destination).unwrap(), b"user-owned");
}
#[test]
fn native_compiler_failures_are_bounded_and_never_publish_partial_artifacts() {
    use std::time::{Duration, Instant};
    let case = fixture("Add-1");
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    for (script, expected, timeout) in [
        (
            "printf 'compiler-diagnostic' >&2; exit 7\n",
            "compiler-diagnostic",
            Duration::from_secs(5),
        ),
        ("sleep 10\n", "timeout", Duration::from_millis(100)),
        ("exit 0\n", "output", Duration::from_secs(5)),
        (
            "for output do :; done\nprintf 'not-ELF' > \"$output\"\n",
            "ELF",
            Duration::from_secs(5),
        ),
        (
            "head -c 2097152 /dev/zero >&2\n",
            "limit",
            Duration::from_secs(5),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let onnx = directory.path().join("model.onnx");
        fs::write(&onnx, &bytes).unwrap();
        let helper = directory.path().join("compiler.sh");
        fs::write(&helper, script).unwrap();
        let destination = directory.path().join("bundle");
        let options = CompileOptions {
            program: "/bin/sh".into(),
            arguments: vec![helper.into_os_string()],
            timeout,
            ..CompileOptions::default()
        };
        let start = Instant::now();
        let error = compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &options,
            &destination,
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!destination.exists());
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}

#[test]
fn protobuf_pin_cannot_authorize_changed_external_weights() {
    // These small TensorProto/GraphProto messages are independent of the compiler.
    fn bytes(field: u8, value: &[u8]) -> Vec<u8> {
        assert!(value.len() < 128);
        [vec![(field << 3) | 2, value.len() as u8], value.to_vec()].concat()
    }
    let directory = tempfile::tempdir().unwrap();
    let onnx = directory.path().join("external.onnx");
    let weights = directory.path().join("weights.bin");
    let tensor = [
        vec![8, 1, 16, 1], // dims=[1], FLOAT
        bytes(8, b"weight"),
        bytes(
            13,
            &[bytes(1, b"location"), bytes(2, b"weights.bin")].concat(),
        ),
        vec![112, 1], // data_location=EXTERNAL
    ]
    .concat();
    let node = [
        bytes(1, b"weight"),
        bytes(2, b"output"),
        bytes(4, b"Identity"),
    ]
    .concat();
    let output = [
        bytes(1, b"output"),
        bytes(
            2,
            &bytes(1, &[vec![8, 1], bytes(2, &bytes(1, &[8, 1]))].concat()),
        ),
    ]
    .concat();
    let source = bytes(
        7,
        &[bytes(1, &node), bytes(5, &tensor), bytes(12, &output)].concat(),
    );
    fs::write(&onnx, &source).unwrap();
    let pin = format!("{:x}", Sha256::digest(&source));
    let destination = directory.path().join("bundle");
    for value in [1.0f32, 9.0] {
        fs::write(&weights, value.to_le_bytes()).unwrap();
        // General import deliberately retains external-data support.
        let imported = openpilot_model_runtime::compiler_onnx::Model::load(&onnx).unwrap();
        assert_eq!(
            imported.graph.initializers["weight"].data,
            value.to_le_bytes()
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&onnx).unwrap())),
            pin
        );
        let error =
            compile_onnx(&onnx, &pin, &CompileOptions::default(), &destination).unwrap_err();
        assert!(error.contains("external_data"), "{error}");
        assert!(!destination.exists());
    }
    fs::remove_file(weights).unwrap();
    let error = compile_onnx(&onnx, &pin, &CompileOptions::default(), &destination).unwrap_err();
    assert!(error.contains("external_data"), "{error}");
    assert!(!destination.exists());
}

#[test]
fn typed_preprocessing_matches_independent_original_compiled_captures() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-typed-preprocess.json"
    ))
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = directory.path().join(format!("model-{index}.onnx"));
        fs::write(&onnx, &bytes).unwrap();
        let bundle = directory.path().join(format!("bundle-{index}"));
        compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions::default(),
            &bundle,
        )
        .unwrap();
        let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
        for input in case["inputs"].as_array().unwrap() {
            model
                .write_input(
                    input["name"].as_str().unwrap(),
                    &decode(input["data_hex"].as_str().unwrap()),
                )
                .unwrap();
        }
        model.run();
        let mut output = vec![0; model.output_size("output").unwrap()];
        model.read_output("output", &mut output).unwrap();
        assert_eq!(
            output,
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn actual_driver_first_nine_nodes_match_independent_compiled_output_hashes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-typed-preprocess.json"
    ))
    .unwrap();
    let prefix = &fixture["driver_prefix"];
    let directory = tempfile::tempdir().unwrap();
    let onnx = directory.path().join("driver-first-nine.onnx");
    let bytes = decode(prefix["onnx_hex"].as_str().unwrap());
    fs::write(&onnx, &bytes).unwrap();
    let bundle = directory.path().join("bundle");
    compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &bundle,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
    model
        .write_input(
            "input_img",
            &decode(prefix["input_img"]["repeat_hex"].as_str().unwrap())
                .repeat(prefix["input_img"]["repetitions"].as_u64().unwrap() as usize),
        )
        .unwrap();
    model
        .write_input(
            "calib",
            &decode(prefix["calib"]["data_hex"].as_str().unwrap()),
        )
        .unwrap();
    model.run();
    for expected in prefix["outputs"].as_array().unwrap() {
        let name = expected["name"].as_str().unwrap();
        let mut output = vec![0; model.output_size(name).unwrap()];
        model.read_output(name, &mut output).unwrap();
        assert_eq!(output.len() as u64, expected["bytes"].as_u64().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(output)),
            expected["sha256"].as_str().unwrap(),
            "actual original driver node {name}"
        );
    }
}
