#![cfg(feature = "native-skip-miri")]
//! Immutable independent original node278 and closed packed SiLU/matrix proofs.
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, CompileOptions},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn decode(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn actual_driving_internal_silu_matrix_prefix_matches_cold_original() {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = sha(&fs::read(&library).unwrap());
    assert!(matches!(
        library_sha256.as_str(),
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
            | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
    ));
    let receipt = actual_original(&library_sha256);
    assert_eq!(receipt["library_sha256"], library_sha256);
    assert_eq!(receipt["last_node"], 278);
    assert_eq!(receipt["output_name"], "policy/val_16");
    assert_eq!(receipt["output_shape"], serde_json::json!([1, 9, 512]));
    for key in [
        "cache_disabled",
        "source_before_after_equal",
        "provider_before_after_equal",
        "producer_input_link_exact",
        "returned_output_link_exact",
    ] {
        assert_eq!(receipt[key], true, "original proof {key}");
    }
    assert_eq!(receipt["graph_only"], false);
    assert!(receipt["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 17);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let full_sha = "f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5";
    assert_eq!(
        sha(
            &fs::read(root.join("openpilot/selfdrive/modeld/models/driving_supercombo.onnx"))
                .unwrap()
        ),
        full_sha
    );
    assert_eq!(receipt["source_model_sha256"], full_sha);
    for (name, expected) in receipt["source_sha256"].as_object().unwrap() {
        assert_eq!(
            sha(&fs::read(root.join(name)).unwrap()),
            expected.as_str().unwrap()
        );
    }
    for (hash, program) in receipt["programs"].as_object().unwrap() {
        assert_eq!(hash.len(), 64);
        assert!(program["source_signature"]
            .as_str()
            .unwrap()
            .starts_with("define void @"));
        assert_eq!(
            program["object_sha256"],
            receipt["provider_real_object_sha256"][hash]
        );
        assert_eq!(program["object_sha256"].as_str().unwrap().len(), 64);
    }
    let boundaries = &receipt["boundaries"];
    let producer_slot = receipt["producer_output_slot"]
        .as_u64()
        .unwrap()
        .to_string();
    let produced = &boundaries["producer_after"]["buffers_hex"][&producer_slot];
    assert_eq!(*produced, boundaries["matrix_before"]["buffers_hex"]["1"]);
    assert_eq!(*produced, boundaries["matrix_after"]["buffers_hex"]["1"]);
    let expected = decode(receipt["output_hex"].as_str().unwrap());
    assert_eq!(expected.len(), 9 * 512 * 4);
    assert_eq!(sha(&expected), receipt["output_sha256"]);
    assert_eq!(
        receipt["output_hex"],
        boundaries["matrix_after"]["buffers_hex"]["0"]
    );
    let source = immutable_model::actual_prefix_source(&receipt);
    let prefix_sha = "f3e4fbc609ec0c4b5c60892b078cdfe9b4b9fe899378357a22ccbc59d7588af5";
    assert_eq!(sha(&source), prefix_sha);
    assert_eq!(receipt["prefix_sha256"], prefix_sha);
    assert_eq!(
        receipt["source_initializer_sha256"]["policy/val_15"]["raw_data_sha256"],
        "c9053c6889bb50e19824bc07cc4137913b4ff000d02b8f15ecec5ec189d126a4"
    );
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
        assert_eq!(sha(raw), receipt["input_sha256"][name]);
    }
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
    for memory_planning in [false, true] {
        eprintln!("actual prefix278 memory={memory_planning}");
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
        .expect("actual internal SiLU plus terminal matrix prefix must compile");
        // SAFETY: locally compiled immutable artifact, exact input extents and
        // output-sized destination, with synchronous execution.
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        for (name, raw) in &inputs {
            model.write_input(name, raw).unwrap();
        }
        model.run();
        let mut actual = vec![0; model.output_size("policy/val_16").unwrap()];
        model.read_output("policy/val_16", &mut actual).unwrap();
        assert_eq!(
            actual, expected,
            "actual prefix278 memory={memory_planning}"
        );
    }
}

#[path = "support/batched_silu_chain.rs"]
mod packed_producer;

fn actual_original(provider: &str) -> Value {
    let bytes = include_bytes!("fixtures/compiler/original-batched-silu-chain.json");
    selected_original(
        bytes,
        "e358180d167d1f101051daffe3edc6c9dad5b89010395484d09f582a32b1fca9",
        provider,
    )
}
fn selected_original(bytes: &[u8], expected: &str, provider: &str) -> Value {
    assert_eq!(sha(bytes), expected);
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["schema"], 1);
    assert_eq!(fixture["captures"].as_array().unwrap().len(), 2);
    let captures = fixture["captures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|capture| capture["library_sha256"] == provider)
        .collect::<Vec<_>>();
    assert_eq!(
        captures.len(),
        1,
        "exact original capture required for selected provider"
    );
    let receipt = captures[0].clone();
    assert_eq!(
        receipt["original_receipt_sha256"].as_str().unwrap().len(),
        64
    );
    receipt
}
#[path = "support/batched_silu_chain_model.rs"]
mod immutable_model;
