#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn whole_concat_slice_bound_output_compiles_and_reuses_two_frames() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-concat-slice-bound18.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 4);
    assert_eq!(fixture["source_sha256"].as_object().unwrap().len(), 13);
    assert_eq!(fixture["cache_disabled"], true);
    for case in fixture["cases"].as_array().unwrap() {
        let source = decode(case["onnx_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["onnx_sha256"]
        );
        for offline_bindings in [false, true] {
            for memory_planning in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("model.onnx");
                let bundle = directory.path().join("bundle");
                fs::write(&path, &source).unwrap();
                let compilation = compile_onnx(
                    &path,
                    case["onnx_sha256"].as_str().unwrap(),
                    &CompileOptions {
                        offline_bindings,
                        memory_planning,
                        ..CompileOptions::default()
                    },
                    &bundle,
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "{} offline={offline_bindings} memory={memory_planning}: {e}",
                        case["name"]
                    )
                });
                assert_eq!(
                    compilation.graph.inputs.len(),
                    if case["constant"].as_bool().unwrap() {
                        2
                    } else {
                        3
                    }
                );
                assert!(!compilation.graph.kernels.is_empty(), "identity output needs a deliberate executable copy, not a permissive empty artifact");
                // SAFETY: trusted immutable artifact freshly compiled from pinned public source.
                let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
                assert_eq!(case["frames"].as_array().unwrap().len(), 2);
                for frame in case["frames"].as_array().unwrap() {
                    for (name, raw) in frame["inputs"].as_object().unwrap() {
                        model
                            .write_input(name, &decode(raw.as_str().unwrap()))
                            .unwrap();
                    }
                    model.run();
                    for (name, expected) in frame["outputs"].as_object().unwrap() {
                        let expected = decode(expected.as_str().unwrap());
                        let mut actual = vec![0; model.output_size(name).unwrap()];
                        model.read_output(name, &mut actual).unwrap();
                        assert_eq!(
                            actual, expected,
                            "{} {name} offline={offline_bindings} memory={memory_planning}",
                            case["name"]
                        );
                    }
                }
            }
        }
    }
}
