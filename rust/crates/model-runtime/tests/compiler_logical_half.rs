#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    compiler_onnx::{lower::LoweredGraph, Model},
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
    serde_json::from_str(include_str!("fixtures/compiler/original-logical-half.json")).unwrap()
}
#[test]
fn logical_half_float_exports_match_independent_original_without_extra_rounding() {
    for case in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["reject"] == true)
    {
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let parsed = Model::parse(&bytes).unwrap();
        assert_eq!(parsed.graph.outputs[0].dtype, 1);
        let lowered = LoweredGraph::lower_offline(&parsed).unwrap();
        assert_eq!(lowered.plans[lowered.outputs["output"]].ty.dtype, 1);
        for offline in [false, true] {
            for memory in [false, true] {
                run_original(case, offline, memory, false);
            }
        }
    }
}
fn run_original(case: &serde_json::Value, offline: bool, memory: bool, multiple: bool) {
    let temp = tempfile::tempdir().unwrap();
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let path = temp.path().join("model.onnx");
    fs::write(&path, &bytes).unwrap();
    let dest = temp.path().join("bundle");
    compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: offline,
            memory_planning: memory,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    model
        .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
        .unwrap();
    model.run();
    let outputs = if multiple {
        case["outputs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(n, v)| (n.as_str(), v["hex"].as_str().unwrap()))
            .collect::<Vec<_>>()
    } else {
        vec![("output", case["expected_hex"].as_str().unwrap())]
    };
    for (name, expected) in outputs {
        let mut actual = vec![0; model.output_size(name).unwrap()];
        model.read_output(name, &mut actual).unwrap();
        assert_eq!(
            actual,
            decode(expected),
            "{} {name} offline={offline} memory={memory}",
            case["name"]
        );
    }
}
#[test]
fn selected_physical_half_output_changes_downstream_float_reads_like_original() {
    for case in fixture()["physical_profile"]["records"].as_array().unwrap() {
        for memory in [false, true] {
            run_original(case, false, memory, true);
        }
    }
}
#[test]
fn verified_half_io_exact_casts_and_explicit_image_storage_remain_original_equivalent() {
    for case in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["reject"] == false)
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: case["offline_bindings"].as_bool().unwrap(),
                ..CompileOptions::default()
            },
            &dest,
        )
        .unwrap();
        let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
        model.run();
        let mut bytes = vec![0; model.output_size("output").unwrap()];
        model.read_output("output", &mut bytes).unwrap();
        assert_eq!(
            bytes,
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn proven_half_concat_matches_original_while_half_relu_remains_rejected() {
    for case in fixture()["pending_conditional"].as_array().unwrap() {
        if case["name"] == "Concat-held-False" {
            for memory in [false, true] {
                run_original(case, false, memory, true);
            }
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: false,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(result.is_err(), "{}", case["name"]);
        assert!(!dest.exists());
    }
}

#[test]
fn infinite_scalar_divisors_preserve_original_onnx_export_zero_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-infinite-divisors.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        for memory in [false, true] {
            run_original(
                case,
                case["offline_bindings"].as_bool().unwrap(),
                memory,
                false,
            );
        }
    }
}
