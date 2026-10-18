#![cfg(feature = "native-skip-miri")]
//! Actual final-output-only prefix277 and immutable independent original controls.
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, CompileOptions},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[test]
fn actual_driving_fused_silu_prefix_matches_cold_original() {
    let bytes = include_bytes!("fixtures/compiler/original-batched-silu.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "226d61715c53c3038f577f073c4a7664cdd188aa5af19c144caf42af8d47cdd7"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["captures"].as_array().unwrap().len(), 2);
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    let receipt = fixture["captures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|capture| capture["library_sha256"] == library_sha256)
        .expect("independent cold original required for selected provider");
    assert_eq!(receipt["last_node"], 277);
    assert_eq!(receipt["output_name"], "policy/silu");
    assert_eq!(receipt["cache_disabled"], true);
    assert_eq!(receipt["graph_only"], false);
    assert!(receipt["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 15);
    assert!(receipt["silu_guard_driver_sha256"].as_str().is_some());
    assert_eq!(
        receipt["matrix_program_source_sha256"],
        "df6cea43648a30f3ed60e4278423000aae1932c52b6159f4fac2b3f31e7d9826"
    );
    assert_eq!(
        receipt["matrix_reference_source_sha256"],
        "f08d5069cd695876ed88b5b35f526830b85f50beee7ed870932358f6fc28f596"
    );
    let source = controls::actual_prefix_source(receipt);
    let prefix_sha = "9bb53bcf01486b497900ebd3aae35294df24234d20f44d5183d63d8240a2715a";
    assert_eq!(format!("{:x}", Sha256::digest(&source)), prefix_sha);
    assert_eq!(receipt["prefix_sha256"], prefix_sha);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let full_model =
        fs::read(root.join("openpilot/selfdrive/modeld/models/driving_supercombo.onnx")).unwrap();
    let full_sha = "f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5";
    assert_eq!(format!("{:x}", Sha256::digest(&full_model)), full_sha);
    assert_eq!(receipt["source_model_sha256"], full_sha);
    for (name, expected) in receipt["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(name)).unwrap())),
            expected.as_str().unwrap(),
            "original source {name}"
        );
    }
    assert_eq!(receipt["library_sha256"], library_sha256);
    let config = Config {
        library,
        library_sha256,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "x86-64".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let inputs = [
        ("img", (0u8..=255).collect::<Vec<_>>().repeat(1536)),
        (
            "big_img",
            (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
        ),
        ("desire_pulse", vec![0; 25 * 8 * 4]),
        ("traffic_convention", vec![0; 8]),
        ("features_buffer", vec![0; 24 * 512 * 4]),
        ("action_t", vec![0; 8]),
    ];
    for (name, raw) in &inputs {
        assert_eq!(
            format!("{:x}", Sha256::digest(raw)),
            receipt["input_sha256"][name]
        );
    }
    for memory_planning in [false, true] {
        eprintln!("actual prefix277 memory={memory_planning}");
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prefix.onnx");
        fs::write(&path, &source).unwrap();
        let destination = temp.path().join("bundle");
        compile_onnx(
            &path,
            prefix_sha,
            &CompileOptions {
                offline_bindings: true,
                memory_planning,
                llvm: Some((
                    config.clone(),
                    PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                )),
                ..CompileOptions::default()
            },
            &destination,
        )
        .expect("actual fused MatMul/Add/Sigmoid/Mul prefix must compile");
        // SAFETY: local immutable compilation with exact original input byte
        // counts, synchronous execution, and output-sized destination storage.
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        for (name, raw) in &inputs {
            model.write_input(name, raw).unwrap();
        }
        model.run();
        let mut actual = vec![0; model.output_size("policy/silu").unwrap()];
        model.read_output("policy/silu", &mut actual).unwrap();
        assert_eq!(actual.len(), 9 * 512 * 4);
        assert_eq!(
            format!("{:x}", Sha256::digest(&actual)),
            receipt["output_sha256"],
            "actual prefix277 memory={memory_planning}"
        );
    }
}

#[path = "support/batched_silu.rs"]
mod controls;
