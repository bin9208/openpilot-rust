//! Independent terminal boundary and caller proofs for the combined biased chain.
use super::*;
use openpilot_model_runtime::compiler_backend_cpu::{llvm::compile_isolated, matrix::MatrixLayout};
use std::{path::Path, process::Command, time::Duration};

fn provider() -> Config {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = sha(&fs::read(&library).unwrap());
    assert!(matches!(
        library_sha256.as_str(),
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
            | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
    ));
    Config {
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
    }
}
fn controls(config: &Config) -> Value {
    let bytes =
        include_bytes!("../fixtures/compiler/original-batched-silu-chain-bias-controls.json");
    assert_eq!(
        sha(bytes),
        "fbd08a99e5bbb07c823fea0e5f846d73662386830f4aefc97791574d43799513"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["schema"], 1);
    assert_eq!(fixture["captures"].as_array().unwrap().len(), 2);
    let matches = fixture["captures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["library_sha256"] == config.library_sha256)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "exact original provider required; no skip/fallback"
    );
    let receipt = matches[0].clone();
    assert_eq!(
        receipt["source_sha256"],
        original(&config.library_sha256)["source_sha256"]
    );
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 45);
    for key in [
        "cache_disabled",
        "source_before_after_equal",
        "provider_before_after_equal",
    ] {
        assert_eq!(receipt[key], true, "original proof {key}");
    }
    assert_eq!(receipt["compile_to_obj_calls"], 46);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
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
    }
    receipt
}
fn dims(case: &Value, name: &str) -> Vec<usize> {
    case[name]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect()
}
fn repeat(case: &Value, name: &str, bytes: usize) -> Vec<u8> {
    decode(case[name].as_str().unwrap())
        .into_iter()
        .cycle()
        .take(bytes)
        .collect()
}

#[test]
fn generic_biased_chain_callers_match_all_original_bytes() {
    let config = provider();
    let receipt = controls(&config);
    let mut checked = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        let (source, input) = immutable_model::model(case);
        let expected = decode(case["outputs_hex"]["result"].as_str().unwrap());
        for memory_planning in [false, true] {
            eprintln!("biased chain {} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("source.onnx");
            fs::write(&path, &source).unwrap();
            let destination = temp.path().join("bundle");
            compile_onnx(
                &path,
                case["onnx_sha256"].as_str().unwrap(),
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
            .expect("independent combined biased chain must compile");
            // SAFETY: locally compiled pinned source, exact input extent and
            // output-sized storage, synchronous execution.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model.write_input("a", &input).unwrap();
            model.run();
            let mut actual = vec![0; model.output_size("result").unwrap()];
            model.read_output("result", &mut actual).unwrap();
            assert_eq!(
                actual, expected,
                "{} memory={memory_planning}",
                case["name"]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 38);
}

#[test]
fn held_readers_and_operand_changes_keep_their_distinct_ownership() {
    use openpilot_model_runtime::compiler_onnx::{lower::LoweredGraph, Model};
    let receipt = controls(&provider());
    let mut checked = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] != "final")
    {
        let (source, _) = immutable_model::model(case);
        let model = Model::parse(&source).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let closures =
            openpilot_model_runtime::compiler_backend_cpu::matrix::graph::closures(&graph, &model)
                .unwrap();
        let reduction = graph.names["terminal_matrix"];
        if case["variant"] == "held_terminal_matrix" {
            assert!(graph.outputs.values().any(|id| *id == reduction));
            // The previously proven bare held matrix can still own its
            // reduction. The later bias must not absorb that held output.
            assert!(
                !closures
                    .iter()
                    .any(|c| c.reduction == reduction && c.bias.is_some()),
                "{}",
                case["name"]
            );
        } else {
            assert!(
                !closures.iter().any(|c| c.packed_silu),
                "{} must not acquire the packed producer",
                case["name"]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 26);
}

#[repr(C, align(32))]
#[derive(Clone)]
struct Block([u8; 32]);
fn aligned(bytes: &[u8]) -> Vec<Block> {
    bytes
        .chunks(32)
        .map(|raw| {
            let mut block = Block([0; 32]);
            block.0[..raw.len()].copy_from_slice(raw);
            block
        })
        .collect()
}
fn build(config: &Config, source: &str, temp: &Path) {
    let object = compile_isolated(
        Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        config,
        source,
        Duration::from_secs(30),
    )
    .unwrap();
    fs::write(temp.join("kernel.o"), object.object).unwrap();
    assert!(Command::new("cc")
        .args(["-shared", "-Wl,-z,defs"])
        .arg(temp.join("kernel.o"))
        .arg("-o")
        .arg(temp.join("kernel.so"))
        .status()
        .unwrap()
        .success());
}
fn invoke(temp: &Path, inputs: &[Vec<u8>], bytes: usize, workers: usize) -> Vec<u8> {
    let mut buffers = vec![aligned(&vec![0xa3; bytes])];
    buffers.extend(inputs.iter().map(|v| aligned(v)));
    let ptrs = buffers
        .iter_mut()
        .map(|v| v.as_mut_ptr().cast::<std::ffi::c_void>())
        .collect::<Vec<_>>();
    // SAFETY: disjoint32-byte aligned buffers matching this generated artifact,
    // synchronous calls, and a library retained through all invocations.
    unsafe {
        let library = libloading::Library::new(temp.join("kernel.so")).unwrap();
        let call = library
            .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(b"op_kernel_0\0")
            .unwrap();
        for worker in 0..workers {
            call(ptrs.as_ptr(), &(worker as i32));
        }
    }
    buffers[0].iter().flat_map(|v| v.0).take(bytes).collect()
}
fn packed_weights(layout: &MatrixLayout, weights: &[u8], original_padding: &str) -> Vec<u8> {
    let groups = layout.output_channels / 4;
    let mut packed = vec![0; layout.weight_elements * 2];
    let stride = packed.len() / groups;
    let defined = layout.input_channels * 4 * 2;
    let padding = decode(original_padding);
    let width = stride - defined;
    assert_eq!(padding.len(), groups * width);
    for row in 0..groups {
        packed[row * stride + defined..(row + 1) * stride]
            .copy_from_slice(&padding[row * width..(row + 1) * width]);
    }
    layout.pack_weights_into(weights, &mut packed).unwrap();
    packed
}

#[test]
fn generic_terminal_four_buffer_artifacts_match_original_boundaries() {
    let config = provider();
    let receipt = controls(&config);
    let mut kernels = std::collections::BTreeMap::new();
    let mut checked = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        let x = dims(case, "producer_shape");
        let w = dims(case, "next_weight_shape");
        let layout = MatrixLayout::new(&x, &w).unwrap();
        let (artifact, temp) = kernels.entry((x, w)).or_insert_with(|| {
            let artifact = layout
                .broadcast_bias_artifact("terminal", "op_kernel_0", 16)
                .unwrap();
            let temp = tempfile::tempdir().unwrap();
            build(&config, &artifact.source, temp.path());
            (artifact, temp)
        });
        let matching = case["checkpoints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["call"] == case["terminal_call"])
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        let boundary = matching[0];
        assert_eq!(boundary["globals"], serde_json::json!([0, 1, 2, 3]));
        assert_eq!(boundary["outs"], serde_json::json!([0]));
        assert_eq!(boundary["ins"], serde_json::json!([1, 2, 3]));
        let inputs = vec![
            decode(boundary["terminal_input_hex"].as_str().unwrap()),
            packed_weights(
                &layout,
                &repeat(
                    case,
                    "next_weight_pattern_hex",
                    layout.input_channels * layout.output_channels * 2,
                ),
                boundary["weight_padding_hex"].as_str().unwrap(),
            ),
            repeat(case, "next_bias_pattern_hex", layout.output_channels * 2),
        ];
        for (slot, bytes) in inputs.iter().enumerate() {
            assert_eq!(
                sha(bytes),
                boundary["buffers_sha256"][slot + 1],
                "{} input{slot}",
                case["name"]
            );
            assert_eq!(bytes.len(), artifact.bytes[slot + 1]);
        }
        assert_eq!(boundary["buffer_bytes"], serde_json::json!(artifact.bytes));
        let expected = decode(case["outputs_hex"]["result"].as_str().unwrap());
        assert_eq!(sha(&expected), boundary["buffers_sha256"][0]);
        let actual = invoke(temp.path(), &inputs, artifact.bytes[0], artifact.workers);
        assert_eq!(actual, expected, "{}", case["name"]);
        checked += 1;
    }
    assert_eq!(checked, 19);
}

#[test]
fn actual_terminal_four_buffer_artifact_matches_original_boundary() {
    use openpilot_model_runtime::compiler_onnx::Model;
    let config = provider();
    let receipt = original(&config.library_sha256);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let bytes =
        fs::read(root.join("openpilot/selfdrive/modeld/models/driving_supercombo.onnx")).unwrap();
    assert_eq!(sha(&bytes), receipt["source_model_sha256"]);
    let model = Model::parse(&bytes).unwrap();
    let layout = MatrixLayout::new(&[1, 9, 512], &[512, 512]).unwrap();
    let artifact = layout
        .broadcast_bias_artifact("terminal", "op_kernel_0", 16)
        .unwrap();
    let boundary = &receipt["boundaries"]["matrix_before"];
    let bias_name = "policy/policy_model.temporal_summarizer._feats_encode.2.bias";
    let inputs = vec![
        decode(boundary["buffers_hex"]["1"].as_str().unwrap()),
        packed_weights(
            &layout,
            &model.graph.initializers["policy/val_15"].data,
            receipt["terminal_weight_padding_hex"].as_str().unwrap(),
        ),
        model.graph.initializers[bias_name].data.clone(),
    ];
    for (slot, bytes) in inputs.iter().enumerate() {
        assert_eq!(
            sha(bytes),
            boundary["buffers_sha256"][(slot + 1).to_string()]
        );
        assert_eq!(bytes.len(), artifact.bytes[slot + 1]);
    }
    let temp = tempfile::tempdir().unwrap();
    build(&config, &artifact.source, temp.path());
    let actual = invoke(temp.path(), &inputs, artifact.bytes[0], artifact.workers);
    assert_eq!(actual, decode(receipt["output_hex"].as_str().unwrap()));
}
