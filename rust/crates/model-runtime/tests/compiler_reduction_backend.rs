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
        "fixtures/compiler/original-reduction-schedule.json"
    ))
    .unwrap()
}
fn execute(case: &serde_json::Value, offline: bool) -> Vec<u8> {
    let directory = tempfile::tempdir().unwrap();
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let onnx = directory.path().join("model.onnx");
    fs::write(&onnx, &bytes).unwrap();
    let bundle = directory.path().join("bundle");
    compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: offline,
            ..CompileOptions::default()
        },
        &bundle,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
    model.run();
    let mut output = vec![0; model.output_size("output").unwrap()];
    model.read_output("output", &mut output).unwrap();
    output
}
#[test]
fn optimized_original_reduction_orders_match_native_artifacts() {
    let f = fixture();
    for case in f["cases"].as_array().unwrap() {
        assert_eq!(
            execute(case, false),
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn optimized_original_padded_image_convolutions_match_native_artifacts() {
    let f = fixture();
    for case in f["conv_cases"].as_array().unwrap() {
        assert_eq!(
            execute(case, true),
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn actual_first_driver_image_convolution_matches_every_original_output_byte_hash() {
    let f = fixture();
    let case = &f["actual_driver_conv"];
    let directory = tempfile::tempdir().unwrap();
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let parsed = openpilot_model_runtime::compiler_onnx::Model::parse(&bytes).unwrap();
    let name = &parsed.graph.outputs[0].name;
    let onnx = directory.path().join("model.onnx");
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
        .write_input("input_img", &(0u8..=255).collect::<Vec<_>>().repeat(5400))
        .unwrap();
    model.run();
    let mut output = vec![0; model.output_size(name).unwrap()];
    model.read_output(name, &mut output).unwrap();
    assert_eq!(output.len() as u64, case["output_bytes"].as_u64().unwrap());
    assert_eq!(
        format!("{:x}", Sha256::digest(output)),
        case["output_sha256"].as_str().unwrap()
    );
}
