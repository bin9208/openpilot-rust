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
fn unproved_logical_half_to_float_export_rejects_before_publication() {
    for case in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["reject"] == true)
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let parsed = Model::parse(&bytes).unwrap();
        assert_eq!(parsed.graph.outputs[0].dtype, 1);
        let lowered = LoweredGraph::lower_offline(&parsed).unwrap();
        assert_eq!(lowered.plans[lowered.outputs["output"]].ty.dtype, 1);
        assert_eq!(case["export_output_dtype"], "dtypes.float");
        assert_eq!(decode(case["expected_hex"].as_str().unwrap()).len(), 64);
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        for offline in [false, true] {
            let dest = temp
                .path()
                .join(if offline { "offline" } else { "physical" });
            let result = compile_onnx(
                &path,
                &format!("{:x}", Sha256::digest(&bytes)),
                &CompileOptions {
                    offline_bindings: offline,
                    ..CompileOptions::default()
                },
                &dest,
            );
            assert!(
                result.is_err(),
                "{} offline={offline} accepted unproved virtual-half export",
                case["name"]
            );
            assert!(!dest.exists());
            assert!(result.err().unwrap().contains("logical-half"));
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
