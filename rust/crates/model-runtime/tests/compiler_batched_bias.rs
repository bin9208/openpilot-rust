#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, matrix::MatrixLayout, CompileOptions},
    compiler_onnx::{lower::LoweredGraph, Model},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn provider() -> Config {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
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
fn fixture(config: &Config) -> Value {
    let bytes = include_bytes!("fixtures/compiler/original-batched-bias-controls.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "de5b1c7d102495be9f42220c429d917f0721a2ad36dc9c9893a9d8434316ae6d"
    );
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(value["captures"].as_array().unwrap().len(), 2);
    let capture = value["captures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["library_sha256"] == config.library_sha256)
        .expect("independent original evidence required for this provider")
        .clone();
    assert_eq!(capture["cache_disabled"], true);
    assert!(capture["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(capture["cases"].as_array().unwrap().len(), 24);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, sha) in capture["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(name)).unwrap())),
            sha.as_str().unwrap(),
            "original source {name}"
        );
    }
    capture
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn vi(mut n: usize) -> Vec<u8> {
    let mut out = vec![];
    while n > 127 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn field(n: usize, value: usize) -> Vec<u8> {
    [vi(n * 8), vi(value)].concat()
}
fn message(n: usize, bytes: &[u8]) -> Vec<u8> {
    [vi(n * 8 + 2), vi(bytes.len()), bytes.to_vec()].concat()
}
fn value(field_id: usize, name: &str, shape: &[usize]) -> Vec<u8> {
    let dims = shape
        .iter()
        .flat_map(|n| message(1, &field(1, *n)))
        .collect::<Vec<_>>();
    message(
        field_id,
        &[
            message(1, name.as_bytes()),
            message(2, &message(1, &[field(1, 1), message(2, &dims)].concat())),
        ]
        .concat(),
    )
}
fn model(case: &Value) -> (Vec<u8>, Vec<u8>) {
    let dims = |key: &str| {
        case[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>()
    };
    let x = dims("input_shape");
    let w = dims("weight_shape");
    let y = dims("output_shape");
    let repeat = |key: &str, bytes: usize| {
        decode(case[key].as_str().unwrap())
            .into_iter()
            .cycle()
            .take(bytes)
            .collect::<Vec<_>>()
    };
    let weights = repeat("weight_pattern_hex", w.iter().product::<usize>() * 2);
    let bias = repeat("bias_pattern_hex", w[1] * 2);
    let tensor = |name: &str, shape: &[usize], raw: &[u8]| {
        message(
            5,
            &[
                shape.iter().flat_map(|n| field(1, *n)).collect::<Vec<_>>(),
                field(2, 10),
                message(8, name.as_bytes()),
                message(9, raw),
            ]
            .concat(),
        )
    };
    let node = |op: &str, inputs: &[&str], output: &str| {
        message(
            1,
            &[
                inputs
                    .iter()
                    .flat_map(|s| message(1, s.as_bytes()))
                    .collect::<Vec<_>>(),
                message(2, output.as_bytes()),
                message(4, op.as_bytes()),
            ]
            .concat(),
        )
    };
    let mut nodes = [
        node("MatMul", &["a", "w"], "matrix"),
        node("Add", &["matrix", "bias"], "linear"),
    ]
    .concat();
    let mut outputs = value(12, "linear", &y);
    match case["variant"].as_str().unwrap() {
        "final" => {}
        "published" => outputs.extend(value(12, "matrix", &y)),
        "shared" => {
            nodes.extend(node("Relu", &["matrix"], "shared"));
            outputs.extend(value(12, "shared", &y));
        }
        _ => panic!("unknown original context"),
    }
    let data = [
        message(8, &field(2, 20)),
        message(
            7,
            &[
                nodes,
                tensor("w", &w, &weights),
                tensor("bias", &[w[1]], &bias),
                value(11, "a", &x),
                outputs,
            ]
            .concat(),
        ),
    ]
    .concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        case["onnx_sha256"].as_str().unwrap()
    );
    let words = case["input_words"].as_array().unwrap();
    let input = words
        .iter()
        .cycle()
        .take(x.iter().product())
        .flat_map(|v| (v.as_u64().unwrap() as u32).to_le_bytes())
        .collect();
    (data, input)
}

#[test]
fn original_exceptional_broadcast_bias_whole_callers_are_exact() {
    let config = provider();
    let f = fixture(&config);
    let mut exact = 0;
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        let (source, input) = model(case);
        for memory_planning in [false, true] {
            eprintln!("{} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("model.onnx");
            fs::write(&path, &source).unwrap();
            let destination = temp.path().join("bundle");
            let compilation = compile_onnx(
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
            .unwrap();
            assert_eq!(compilation.graph.backend, "cpu-llvm");
            // SAFETY: locally compiled immutable bundle; exact original input
            // byte count, synchronous execution and output-sized destination.
            let mut native = unsafe { CpuModel::load(&destination) }.unwrap();
            native.write_input("a", &input).unwrap();
            native.run();
            let mut actual = vec![0; native.output_size("linear").unwrap()];
            native.read_output("linear", &mut actual).unwrap();
            assert_eq!(
                actual,
                decode(case["outputs_hex"]["linear"].as_str().unwrap()),
                "{} memory={memory_planning}",
                case["name"]
            );
            exact += 1;
        }
    }
    assert_eq!(exact, 36);
}

#[test]
fn published_and_reused_matrix_contexts_do_not_acquire_final_bias_fusion() {
    let config = provider();
    let f = fixture(&config);
    let mut checked = 0;
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] != "final")
    {
        let (source, _) = model(case);
        let source = Model::parse(&source).unwrap();
        let lowered = LoweredGraph::lower_offline(&source).unwrap();
        let selected = openpilot_model_runtime::compiler_backend_cpu::matrix::graph::closures(
            &lowered, &source,
        )
        .unwrap();
        assert!(
            !selected.iter().any(|c| c.bias.is_some()),
            "{} requires separately proven materialization",
            case["name"]
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
}

#[test]
fn old_single_row_terminal_entrypoint_retains_its_fence() {
    for shape in [&[1, 9, 512][..], &[3, 512][..]] {
        let layout = MatrixLayout::new(shape, &[512, 512]).unwrap();
        let error = layout
            .biased_artifact("old_terminal", "op_kernel_0", 16)
            .err()
            .expect("the old entrypoint must retain its single-row contract");
        assert!(error.contains("single-row"));
    }
}
