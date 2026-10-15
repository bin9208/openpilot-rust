#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use sha2::{Digest, Sha256};
use std::fs;
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "fixtures/compiler/original-storage-alias.json"
    ))
    .unwrap()
}
fn run(case: &serde_json::Value, actual: bool) -> Vec<(String, Vec<u8>)> {
    let temp = tempfile::tempdir().unwrap();
    let source = decode(case["onnx_hex"].as_str().unwrap());
    let path = temp.path().join("model.onnx");
    fs::write(&path, &source).unwrap();
    let dest = temp.path().join("bundle");
    let compilation = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    if actual {
        model
            .write_input("input_img", &(0u8..=255).collect::<Vec<_>>().repeat(5400))
            .unwrap();
    } else {
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
    }
    model.run();
    compilation
        .graph
        .outputs
        .iter()
        .map(|out| {
            let mut bytes = vec![0; model.output_size(&out.name).unwrap()];
            model.read_output(&out.name, &mut bytes).unwrap();
            (out.name.clone(), bytes)
        })
        .collect()
}
#[test]
fn original_image_storage_alias_respects_computed_inputs_padding_and_held_outputs() {
    for case in fixture()["cases"].as_array().unwrap() {
        for (name, bytes) in run(case, false) {
            assert_eq!(
                bytes,
                decode(case["outputs"][&name].as_str().unwrap()),
                "{} {name}",
                case["name"]
            );
        }
    }
}
#[test]
fn actual_driver_true_prefix_storage_alias_matches_original_not_forced_diagnostics() {
    for case in fixture()["actual_prefixes"].as_array().unwrap() {
        let outputs = run(case, true);
        assert_eq!(outputs.len(), 1);
        let bytes = &outputs[0].1;
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            case["original"]["sha256"].as_str().unwrap(),
            "prefix{}",
            case["index"]
        );
        assert_ne!(case["original"]["sha256"], case["diagnostic"]["sha256"]);
    }
}
#[test]
fn shared_view_image_alias_rejects_before_artifact_publication() {
    for case in fixture()["unsupported_views"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &source).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&source)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(
            result.is_err(),
            "{} accepted unimplemented shared view alias",
            case["name"]
        );
        assert!(!dest.exists());
        assert!(result.err().unwrap().contains("inverse movement"));
    }
}
#[test]
fn underlying_view_consumers_and_dynamic_weights_reject_without_publication() {
    for case in fixture()["review_cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let source = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &source).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&source)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(
            result.is_err(),
            "{} accepted unsupported alias",
            case["name"]
        );
        assert!(!dest.exists());
        let error = result.err().unwrap();
        assert!(
            error.contains("inverse movement") || error.contains("dynamic weight"),
            "{error}"
        );
    }
}
