#![cfg(feature = "native-skip-miri")]
//! Immutable independent original node279 positive admission regression.
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
fn actual_driving_internal_silu_biased_matrix_unplanned_matches_cold_original() {
    actual(false);
}
#[test]
fn actual_driving_internal_silu_biased_matrix_planned_matches_cold_original() {
    actual(true);
}
fn actual(memory_planning: bool) {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = sha(&fs::read(&library).unwrap());
    assert!(matches!(
        library_sha256.as_str(),
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
            | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
    ));
    let receipt = original(&library_sha256);
    assert_eq!(receipt["library_sha256"], library_sha256);
    assert_eq!(receipt["last_node"], 279);
    assert_eq!(receipt["output_name"], "policy/linear_1");
    assert_eq!(receipt["output_shape"], serde_json::json!([1, 9, 512]));
    for key in [
        "cache_disabled",
        "source_before_after_equal",
        "provider_before_after_equal",
        "input_owner_links_exact",
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
    let terminal = &receipt["calls"][receipt["terminal_call_index"].as_u64().unwrap() as usize];
    assert_eq!(terminal["op"], "PROGRAM");
    assert_eq!(terminal["regions"]["0"], receipt["returned_output_region"]);
    assert_eq!(
        terminal["source"],
        receipt["terminal_program_source_sha256"]
    );
    assert_eq!(terminal["globals"], serde_json::json!([0, 1, 2, 3]));
    assert_eq!(terminal["ins"], serde_json::json!([1, 2, 3]));
    assert_eq!(terminal["outs"], serde_json::json!([0]));
    assert_eq!(receipt["terminal_call_index"], 754);
    assert_eq!(receipt["input_owners"]["1"]["call_index"], 751);
    assert_eq!(
        boundaries["input_owner_after_751"]["buffers_hex"]["0"],
        boundaries["matrix_before"]["buffers_hex"]["1"]
    );
    assert_eq!(
        boundaries["matrix_before"]["buffers_hex"]["1"],
        boundaries["matrix_after"]["buffers_hex"]["1"]
    );
    for (slot, owner) in receipt["input_owners"].as_object().unwrap() {
        let before = &boundaries["matrix_before"]["buffers_sha256"][slot];
        assert_eq!(*before, boundaries["matrix_after"]["buffers_sha256"][slot]);
        if owner.is_null() {
            continue;
        }
        assert_eq!(
            owner["op"],
            receipt["calls"][owner["call_index"].as_u64().unwrap() as usize]["op"]
        );
        assert_eq!(owner["region"], terminal["regions"][slot]);
        let key = format!(
            "input_owner_after_{}",
            owner["call_index"].as_u64().unwrap()
        );
        let output_slot = owner["output_slot"].as_u64().unwrap().to_string();
        assert_eq!(*before, boundaries[&key]["buffers_sha256"][&output_slot]);
    }
    let expected = decode(receipt["output_hex"].as_str().unwrap());
    assert_eq!(expected.len(), 9 * 512 * 4);
    assert_eq!(sha(&expected), receipt["output_sha256"]);
    assert_eq!(
        receipt["output_hex"],
        boundaries["matrix_after"]["buffers_hex"]["0"]
    );
    let source = immutable_model::actual_prefix_source(&receipt);
    let prefix_sha = "87127718975d87c2cfa3dd4d79bfc42f2922e11a6c4621257b52e0ab322c4ca6";
    assert_eq!(sha(&source), prefix_sha);
    assert_eq!(receipt["prefix_sha256"], prefix_sha);
    assert_eq!(
        receipt["source_initializer_sha256"]["policy/val_15"]["raw_data_sha256"],
        "c9053c6889bb50e19824bc07cc4137913b4ff000d02b8f15ecec5ec189d126a4"
    );
    let bias = &receipt["source_initializer_sha256"]
        ["policy/policy_model.temporal_summarizer._feats_encode.2.bias"];
    assert_eq!(bias["raw_data_bytes"], 1024);
    assert_eq!(
        boundaries["matrix_before"]["buffers_sha256"]["3"],
        bias["raw_data_sha256"]
    );
    assert_eq!(
        bias["raw_data_sha256"],
        "e50043b3ef9abcd6b475d2f75ca25c57e679a47d172e50e79337794d060a1e83"
    );
    assert_eq!(
        bias["tensor_proto_sha256"],
        "1df8c2437ac8cf026d581ca5f819752c24ad7260cfbe1893d7d5832ee2d7ef0c"
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
    eprintln!("actual prefix279 memory={memory_planning}");
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
    .expect("actual internal SiLU plus terminal biased matrix prefix must compile");
    // SAFETY: locally compiled immutable artifact, exact input extents and
    // output-sized destination, with synchronous execution.
    let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
    for (name, raw) in &inputs {
        model.write_input(name, raw).unwrap();
    }
    model.run();
    let mut actual = vec![0; model.output_size("policy/linear_1").unwrap()];
    model.read_output("policy/linear_1", &mut actual).unwrap();
    assert_eq!(
        actual, expected,
        "actual prefix279 memory={memory_planning}"
    );
}

fn original(provider: &str) -> Value {
    let bytes = include_bytes!("fixtures/compiler/original-batched-silu-chain-bias.json");
    assert_eq!(
        sha(bytes),
        "78a57ee022458a1033f65d793878c85e6fd802248d9a4c2e7142d6fbb5d15bae"
    );
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
        "independent original capture for exact provider required; no fallback or skip"
    );
    let receipt = captures[0].clone();
    let expected = match provider {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            "5900102ac01b81c0f44053431bb4dc3f310a8978fec1345a520b398c607c4442"
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            "09b519ce77111ce1922e19d3411296dc8550b84d4033a4177fe257fa8e17159f"
        }
        _ => panic!("unproven provider"),
    };
    assert_eq!(receipt["original_receipt_sha256"], expected);
    let old = include_bytes!("fixtures/compiler/original-batched-silu-chain.json");
    assert_eq!(
        sha(old),
        "e358180d167d1f101051daffe3edc6c9dad5b89010395484d09f582a32b1fca9"
    );
    let proof: Value = serde_json::from_slice(old).unwrap();
    assert_eq!(
        receipt["source_sha256"],
        proof["captures"][0]["source_sha256"]
    );
    receipt
}
#[path = "support/batched_silu_chain_bias_model.rs"]
mod immutable_model;

#[path = "support/batched_silu_chain_bias_controls.rs"]
mod biased_controls;
