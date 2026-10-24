//! Physical packed producer proof; no candidate output supplies expectations.
use super::*;
use openpilot_model_runtime::compiler_backend_cpu::{llvm::compile_isolated, matrix::MatrixLayout};
use std::{path::Path, process::Command, time::Duration};

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
fn build(config: &Config, source: &str, temp: &Path) {
    let built = compile_isolated(
        Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        config,
        source,
        Duration::from_secs(30),
    )
    .unwrap();
    fs::write(temp.join("kernel.o"), built.object).unwrap();
    assert!(Command::new("cc")
        .arg("-shared")
        .arg("-Wl,-z,defs")
        .arg(temp.join("kernel.o"))
        .arg("-o")
        .arg(temp.join("kernel.so"))
        .status()
        .unwrap()
        .success());
}
fn invoke(temp: &Path, buffers: &mut [Vec<Block>], workers: usize) {
    let ptrs = buffers
        .iter_mut()
        .map(|buffer| buffer.as_mut_ptr().cast::<std::ffi::c_void>())
        .collect::<Vec<_>>();
    // SAFETY: exact locally generated artifact with disjoint aligned buffers,
    // synchronous calls and a library that outlives every invocation.
    unsafe {
        let library = libloading::Library::new(temp.join("kernel.so")).unwrap();
        let call = library
            .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(b"op_kernel_0\0")
            .unwrap();
        for worker in 0..workers {
            call(ptrs.as_ptr(), &(worker as i32));
        }
    }
}
#[test]
fn actual_internal_packed_silu_defined_lanes_match_original_boundary() {
    let config = provider();
    let receipt = actual_original(&config.library_sha256);
    assert_eq!(receipt["library_sha256"], config.library_sha256);
    assert_eq!(receipt["returned_output_link_exact"], true);
    assert_eq!(receipt["producer_input_link_exact"], true);
    let boundary = &receipt["boundaries"]["producer_after"];
    let layout = MatrixLayout::new(&[1, 9, 512], &[512, 512]).unwrap();
    let packed = MatrixLayout::new(&[1, 9, 512], &[512, 4]).unwrap();
    let artifact = layout
        .packed_silu_artifact("packed_silu", "op_kernel_0", 16)
        .unwrap();
    let raw = actual_buffers(&receipt, &layout);
    for (slot, bytes) in raw.iter().enumerate() {
        assert_eq!(sha(bytes), boundary["buffers_sha256"][slot.to_string()]);
    }
    assert_eq!(
        artifact.bytes.to_vec(),
        raw.iter().map(Vec::len).collect::<Vec<_>>()
    );
    let temp = tempfile::tempdir().unwrap();
    build(&config, &artifact.source, temp.path());
    let sentinel = vec![0xa3; artifact.bytes[0]];
    let mut buffers = vec![aligned(&sentinel)];
    buffers.extend(raw[1..].iter().map(|bytes| aligned(bytes)));
    invoke(temp.path(), &mut buffers, artifact.workers);
    let actual = buffers[0]
        .iter()
        .flat_map(|b| b.0)
        .take(raw[0].len())
        .collect::<Vec<_>>();
    // Original producer stores logical lanes; allocation padding is capacity,
    // not an initialized numerical result. Check those lanes independently and
    // retain sentinel bytes in every non-owned padding element.
    for row in 0..9 {
        let first = row * packed.input_row_stride * 2;
        let end = first + 512 * 2;
        assert_eq!(&actual[first..end], &raw[0][first..end], "row {row}");
        let capacity_end = (row + 1) * packed.input_row_stride * 2;
        assert_eq!(&actual[end..capacity_end], &sentinel[end..capacity_end]);
    }
}

fn controls(config: &Config) -> Value {
    let bytes = include_bytes!("../fixtures/compiler/original-batched-silu-chain-controls.json");
    let receipt = selected_original(
        bytes,
        "9ecdf5de1796d4acfebe7146b03401b602f3adf6ff1849527b3d6f28c08304fe",
        &config.library_sha256,
    );
    assert_eq!(receipt["library_sha256"], config.library_sha256);
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 39);
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 17);
    assert_eq!(receipt["source_before_after_equal"], true);
    assert_eq!(receipt["provider_before_after_equal"], true);
    assert_eq!(receipt["cache_disabled"], true);
    assert!(receipt["compile_to_obj_calls"].as_u64().unwrap() > 0);
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
fn source(case: &Value) -> Vec<u8> {
    let (source, _) = immutable_model::model(case);
    assert_eq!(sha(&source), case["onnx_sha256"]);
    source
}
fn dimensions(case: &Value, field: &str) -> Vec<usize> {
    case[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect()
}
#[test]
fn generic_closed_chain_physical_silu_defined_lanes_match_original() {
    let config = provider();
    let receipt = controls(&config);
    let mut checked = 0;
    // Modes change only runtime input/initializer buffers. Reuse compilation
    // only for an identical input/weight shape and this exact packed profile;
    // every original boundary, valid lane and padding assertion still runs.
    let mut kernels = std::collections::BTreeMap::new();
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        eprintln!("packed producer {}", case["name"]);
        let x = dimensions(case, "input_shape");
        let w = dimensions(case, "weight_shape");
        let y = dimensions(case, "producer_shape");
        let layout = MatrixLayout::new(&x, &w).unwrap();
        let packed = MatrixLayout::new(&y, &[w[1], 4]).unwrap();
        let (artifact, temp) = kernels.entry((x.clone(), w.clone())).or_insert_with(|| {
            let artifact = layout
                .packed_silu_artifact("packed_silu", "op_kernel_0", 16)
                .unwrap();
            let temp = tempfile::tempdir().unwrap();
            build(&config, &artifact.source, temp.path());
            (artifact, temp)
        });
        let matching = case["checkpoints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|checkpoint| {
                checkpoint["globals"] == serde_json::json!([0, 1, 2, 3])
                    && checkpoint["outs"] == serde_json::json!([0])
                    && checkpoint["buffer_bytes"] == serde_json::json!(artifact.bytes.to_vec())
                    && receipt["programs"][checkpoint["source_sha256"].as_str().unwrap()]
                        ["source_signature"]
                        .as_str()
                        .unwrap()
                        .lines()
                        .next()
                        .unwrap()
                        .contains("(i16*")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "unique original packed producer {}",
            case["name"]
        );
        let boundary = matching[0];
        let raw = control_buffers(case, boundary, &layout);
        for (slot, bytes) in raw.iter().enumerate() {
            assert_eq!(
                sha(bytes),
                boundary["buffers_sha256"][slot],
                "{} slot {slot}",
                case["name"]
            );
        }
        let sentinel = vec![0xa3; artifact.bytes[0]];
        let mut buffers = vec![aligned(&sentinel)];
        buffers.extend(raw[1..].iter().map(|bytes| aligned(bytes)));
        invoke(temp.path(), &mut buffers, artifact.workers);
        let actual = buffers[0]
            .iter()
            .flat_map(|b| b.0)
            .take(raw[0].len())
            .collect::<Vec<_>>();
        for row in 0..layout.batches * layout.rows {
            let start = row * packed.input_row_stride * 2;
            let end = start + w[1] * 2;
            assert_eq!(
                &actual[start..end],
                &raw[0][start..end],
                "{} row {row}",
                case["name"]
            );
            let capacity_end = (row + 1) * packed.input_row_stride * 2;
            assert_eq!(&actual[end..capacity_end], &sentinel[end..capacity_end]);
        }
        checked += 1;
    }
    assert_eq!(checked, 19);
}
#[test]
fn generic_closed_silu_matrix_callers_match_original() {
    let config = provider();
    let receipt = controls(&config);
    let mut checked = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        let source = source(case);
        let shape = dimensions(case, "input_shape");
        let raw = case["input_words"]
            .as_array()
            .unwrap()
            .iter()
            .cycle()
            .take(shape.iter().product())
            .flat_map(|word| (word.as_u64().unwrap() as u32).to_le_bytes())
            .collect::<Vec<_>>();
        let expected = decode(case["outputs_hex"]["result"].as_str().unwrap());
        for memory_planning in [false, true] {
            eprintln!("{} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let model_path = temp.path().join("model.onnx");
            fs::write(&model_path, &source).unwrap();
            let destination = temp.path().join("bundle");
            compile_onnx(
                &model_path,
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
            .expect("independent closed matrix/SiLU/matrix caller must compile");
            // SAFETY: locally compiled immutable bundle, exact input extent,
            // synchronous execution and output-sized destination storage.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model.write_input("a", &raw).unwrap();
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
fn held_extra_readers_and_operand_changes_do_not_gain_internal_silu_ownership() {
    use openpilot_model_runtime::compiler_onnx::{lower::LoweredGraph, Model};
    let config = provider();
    let receipt = controls(&config);
    let mut checked = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] != "final")
    {
        let model = Model::parse(&source(case)).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let closures =
            openpilot_model_runtime::compiler_backend_cpu::matrix::graph::closures(&graph, &model)
                .unwrap();
        assert!(
            !closures.iter().any(|closure| closure.silu.is_some()
                && !graph.outputs.values().any(|id| *id == closure.output)),
            "{} must preserve held/reader/operand source contract",
            case["name"]
        );
        // The original held-SiLU final-f32 profile remains independently valid;
        // this guard concerns only ownership of an unheld packed producer.
        checked += 1;
    }
    assert_eq!(checked, 20);
}

fn packed_buffers(
    layout: &MatrixLayout,
    input: &[u8],
    weights: &[u8],
    bias: Vec<u8>,
    boundary: &Value,
) -> [Vec<u8>; 3] {
    let mut packed_input = vec![0; layout.input_elements * 2];
    seed_padding(
        &mut packed_input,
        layout.batches * layout.rows,
        layout.input_channels,
        boundary["input_padding_hex"].as_str().unwrap(),
    );
    layout.pack_input_into(input, &mut packed_input).unwrap();
    let mut packed_weights = vec![0; layout.weight_elements * 2];
    seed_padding(
        &mut packed_weights,
        layout.output_channels / 4,
        layout.input_channels * 4,
        boundary["weight_padding_hex"].as_str().unwrap(),
    );
    layout
        .pack_weights_into(weights, &mut packed_weights)
        .unwrap();
    [packed_input, packed_weights, bias]
}
fn actual_buffers(receipt: &Value, layout: &MatrixLayout) -> Vec<Vec<u8>> {
    use openpilot_model_runtime::compiler_onnx::Model;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let path = root
        .join("openpilot/selfdrive/modeld/models")
        .join(receipt["source_model"].as_str().unwrap());
    let source = fs::read(path).unwrap();
    assert_eq!(sha(&source), receipt["source_model_sha256"]);
    let model = Model::parse(&source).unwrap();
    let tensor = |node: &str| {
        let index = receipt[node].as_u64().unwrap() as usize;
        let name = &model.graph.nodes[index].inputs[1];
        let tensor = &model.graph.initializers[name];
        assert_eq!(tensor.dtype, 10);
        assert_eq!(
            sha(&tensor.data),
            receipt["source_initializer_sha256"][name]["raw_data_sha256"]
        );
        tensor.data.clone()
    };
    let weights = tensor("producer_weight_node");
    let bias = tensor("producer_bias_node");
    assert_eq!(bias.len(), layout.output_channels * 2);
    let mut packed_weights = vec![0; layout.weight_elements * 2];
    seed_padding(
        &mut packed_weights,
        layout.output_channels / 4,
        layout.input_channels * 4,
        receipt["producer_weight_padding_hex"].as_str().unwrap(),
    );
    layout
        .pack_weights_into(&weights, &mut packed_weights)
        .unwrap();
    let boundary = &receipt["boundaries"]["producer_after"];
    vec![
        decode(boundary["buffers_hex"]["0"].as_str().unwrap()),
        decode(boundary["buffers_hex"]["1"].as_str().unwrap()),
        packed_weights,
        bias,
    ]
}
fn control_buffers(case: &Value, boundary: &Value, layout: &MatrixLayout) -> Vec<Vec<u8>> {
    let (_, input) = immutable_model::model(case);
    let repeat = |key: &str, len: usize| {
        decode(case[key].as_str().unwrap())
            .into_iter()
            .cycle()
            .take(len)
            .collect::<Vec<_>>()
    };
    let weights = repeat(
        "weight_pattern_hex",
        layout.input_channels * layout.output_channels * 2,
    );
    let bias = repeat("bias_pattern_hex", layout.output_channels * 2);
    let mut raw = vec![decode(boundary["producer_output_hex"].as_str().unwrap())];
    raw.extend(packed_buffers(layout, &input, &weights, bias, boundary));
    raw
}

/// Original input/weight allocation padding can retain bytes from previous
/// materializations. Preserve that independently captured capacity, reconstruct
/// every defined lane from fixed recipes, then require the original full hash.
/// Padding is not a numerical expected output and is never canonicalized.
fn seed_padding(bytes: &mut [u8], groups: usize, logical_elements: usize, original: &str) {
    assert_eq!(bytes.len() % groups, 0);
    let stride = bytes.len() / groups;
    let defined = logical_elements * 2;
    assert!(defined <= stride);
    let padding = decode(original);
    let width = stride - defined;
    assert_eq!(padding.len(), groups * width);
    for row in 0..groups {
        bytes[row * stride + defined..(row + 1) * stride]
            .copy_from_slice(&padding[row * width..(row + 1) * width]);
    }
}
