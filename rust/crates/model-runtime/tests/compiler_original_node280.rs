#![cfg(feature = "native-skip-miri")]
//! Cold independent original final-output prefix280, including its actual owners.
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
fn original_node280_unplanned_matches_full_ancestor_prefix() {
    actual(false, false);
}
#[test]
fn original_node280_planned_matches_full_ancestor_prefix() {
    actual(true, false);
}
#[test]
fn original_node280_nonzero_desire_unplanned_matches_full_ancestor_prefix() {
    actual(false, true);
}
#[test]
fn original_node280_nonzero_desire_planned_matches_full_ancestor_prefix() {
    actual(true, true);
}

fn actual(memory_planning: bool, nonzero_desire: bool) {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let provider = sha(&fs::read(&library).unwrap());
    assert!(matches!(
        provider.as_str(),
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
            | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
    ));
    let original_bytes: &[u8] = if nonzero_desire {
        include_bytes!("fixtures/compiler/original-node280-nonzero.json")
    } else {
        include_bytes!("fixtures/compiler/original-node280.json")
    };
    assert_eq!(
        sha(original_bytes),
        if nonzero_desire {
            "067818b1bab06bc07c18c67dee9b8f77e824ff371a74237bc0d8e0846b3c63e8"
        } else {
            "e988fa15dcff4ecc16f3be53cc9633f7a16f31faa11c7a107f926574a2ccc6bb"
        }
    );
    let fixture: Value = serde_json::from_slice(original_bytes).unwrap();
    assert_eq!(fixture["schema"], 1);
    assert_eq!(
        fixture["provenance"]["source_commit"],
        if nonzero_desire {
            "ca42e06a490f3bfd1287f0eb0104048726a15aa9"
        } else {
            "6836c733beff11a5493df8dcbe29d8cb5f5a168a"
        }
    );
    assert_eq!(
        fixture["provenance"]["artifact_zip_sha256"],
        if nonzero_desire {
            "efe890b579566c7fe193c8e5fd49517dada3c9ec197275143b972a9e737786f7"
        } else {
            "6c8fdad643aacda597ac164c80c66c0ce3e25f6ab6b08c28882a18930a2f2b9d"
        }
    );
    assert_eq!(fixture["captures"].as_array().unwrap().len(), 2);
    let captures = fixture["captures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["library_sha256"] == provider)
        .collect::<Vec<_>>();
    assert_eq!(
        captures.len(),
        1,
        "exact independently captured provider required"
    );
    let receipt = captures[0];
    let raw_receipt = match (nonzero_desire, provider.starts_with("5c3a")) {
        (false, true) => "8d453af80f005574841af9900c0594169e7ad0592db9ae32e15cf14dc5c770a8",
        (false, false) => "e86950fae78e23520690fc9aa2308c3a91a9730906e0e5186d77b7aff384a7be",
        (true, true) => "2b5a2165f29251a70e971c2d3a77d4d04b4f1754b98ea58a9b486c6c7d408322",
        (true, false) => "4d85f5430e6036f65f2f2622533fdf9580061322317954d5160370451f618757",
    };
    assert_eq!(receipt["original_receipt_sha256"], raw_receipt);
    assert_eq!(
        receipt["guard_driver_sha256"],
        fixture["provenance"]["driver_sha256"]
    );
    assert_eq!(receipt["last_node"], 280);
    assert_eq!(receipt["output_name"], "policy/linear_2");
    assert_eq!(receipt["output_shape"], serde_json::json!([1, 512]));
    assert_eq!(receipt["prefix_bytes"], 48_027_436);
    assert_eq!(receipt["program_calls"], 3);
    assert_eq!(receipt["compile_to_obj_calls_after_execution"], 3);
    assert_eq!(receipt["compile_to_obj_calls_after_output_read"], 3);
    for key in [
        "cache_disabled",
        "source_before_after_equal",
        "provider_before_after_equal",
        "input_owner_links_exact",
    ] {
        assert_eq!(receipt[key], true, "original proof {key}");
    }
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 17);
    assert_eq!(
        receipt["source_initializer_sha256"]
            .as_object()
            .unwrap()
            .len(),
        280
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compiler/node280");
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
    let expected = decode(receipt["output_hex"].as_str().unwrap());
    assert_eq!(expected.len(), 512 * 4);
    assert_eq!(sha(&expected), receipt["output_sha256"]);
    assert_eq!(
        receipt["output_sha256"],
        if nonzero_desire {
            "47989d3b10646cd7933c72366373ebb3f753222bf2fea9dba653c6ba363d8ec1"
        } else {
            "b0b908175ad4849b177048ab2078ad82917e1fc5ef83c886c02dfaa9dc03e77e"
        }
    );
    let terminal = &receipt["terminal_owner"];
    assert_eq!(terminal["call_index"], 7);
    assert_eq!(terminal["op"], "PROGRAM");
    assert_eq!(terminal["region"], receipt["returned_output_region"]);
    assert_eq!(terminal["ins"], serde_json::json!([1, 2, 3]));
    assert_eq!(terminal["outs"], serde_json::json!([0]));
    assert_eq!(
        receipt["boundaries"]["terminal_after"]["buffers_sha256"]["0"],
        receipt["output_sha256"]
    );
    for (slot, index, kind) in [("1", 3, "PROGRAM"), ("2", 6, "PROGRAM"), ("3", 5, "COPY")] {
        let owner = &receipt["input_owners"][slot];
        assert_eq!(owner["call_index"], index);
        assert_eq!(owner["op"], kind);
        assert_eq!(
            owner["region"],
            receipt["boundaries"]["terminal_before"]["regions"][slot]
        );
    }
    let mut inputs = Vec::new();
    for slot in ["1", "2", "3"] {
        let file = if nonzero_desire && slot == "1" {
            "input-1-nonzero.bin".to_owned()
        } else {
            format!("input-{slot}.bin")
        };
        let raw = fs::read(assets.join(file)).unwrap();
        let digest = sha(&raw);
        assert_eq!(
            digest,
            receipt["boundaries"]["terminal_before"]["buffers_sha256"][slot]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            digest,
            receipt["boundaries"]["terminal_after"]["buffers_sha256"][slot]
                .as_str()
                .unwrap()
        );
        let owner = &receipt["input_owners"][slot];
        let key = format!(
            "input_owner_after_{}",
            owner["call_index"].as_u64().unwrap()
        );
        let output_slot = owner["output_slot"].as_u64().unwrap().to_string();
        assert_eq!(
            digest,
            receipt["boundaries"][&key]["buffers_sha256"][&output_slot]
                .as_str()
                .unwrap()
        );
        inputs.push(raw);
    }
    assert_eq!(
        inputs.iter().map(Vec::len).collect::<Vec<_>>(),
        [1600, 1_638_400, 1024]
    );
    let programs = receipt["programs"].as_object().unwrap();
    assert_eq!(programs.len(), 3);
    for (source, program) in programs {
        let ir = fs::read(assets.join(program["source_file"].as_str().unwrap())).unwrap();
        let object = decode(program["object_hex"].as_str().unwrap());
        assert_eq!(sha(&ir), source.as_str());
        assert_eq!(sha(&object), program["object_sha256"].as_str().unwrap());
        assert_eq!(
            program["object_sha256"],
            receipt["provider_real_object_sha256"][source]
        );
    }
    assert!(programs.contains_key(terminal["source"].as_str().unwrap()));
    let source = immutable_model::actual_prefix_source(receipt);
    let prefix_sha = "24818f594a05ec3463d8a62bfbe3be53c5f4ad727bfac7b03e0598550de0ab3f";
    assert_eq!(sha(&source), prefix_sha);
    let desire = if nonzero_desire {
        assert_eq!(
            receipt["desire_pulse_recipe"],
            "25x8 little-endian f32; row i has one at column i%8 and zero elsewhere"
        );
        (0..25)
            .flat_map(|row| {
                (0..8).flat_map(move |column| {
                    (if column == row % 8 { 1.0f32 } else { 0.0f32 }).to_le_bytes()
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![0; 25 * 8 * 4]
    };
    let inputs = [
        ("img", (0u8..=255).collect::<Vec<_>>().repeat(1536)),
        (
            "big_img",
            (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
        ),
        ("desire_pulse", desire),
        ("traffic_convention", vec![0; 8]),
        ("features_buffer", vec![0; 24 * 512 * 4]),
        ("action_t", vec![0; 8]),
    ];
    for (name, raw) in &inputs {
        assert_eq!(sha(raw), receipt["input_sha256"][name]);
    }
    let config = Config {
        library,
        library_sha256: provider,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "x86-64".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("prefix.onnx");
    fs::write(&path, source).unwrap();
    let destination = temp.path().join("bundle");
    compile_onnx(
        &path,
        prefix_sha,
        &CompileOptions {
            offline_bindings: true,
            memory_planning,
            llvm: Some((
                config,
                PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &destination,
    )
    .expect("actual node280 full ancestor prefix must compile");
    // SAFETY: locally compiled immutable artifact with exact input and output extents.
    let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
    for (name, raw) in &inputs {
        model.write_input(name, raw).unwrap();
    }
    model.run();
    let mut actual = vec![0; model.output_size("policy/linear_2").unwrap()];
    model.read_output("policy/linear_2", &mut actual).unwrap();
    assert_eq!(
        actual, expected,
        "actual prefix280 memory={memory_planning} nonzero_desire={nonzero_desire}"
    );
}

#[path = "support/batched_silu_chain_bias_model.rs"]
#[allow(dead_code)]
mod immutable_model;
