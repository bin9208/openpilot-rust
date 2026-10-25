#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, CompileOptions},
    compiler_onnx::Model,
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn decode(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn provider() -> Config {
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    assert!(
        matches!(
            library_sha256.as_str(),
            "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
                | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
        ),
        "unsupported provider identity"
    );
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
#[test]
fn unheld_terminal_normalized_chain_matches_independent_original_all_bytes() {
    let config = provider();
    let bytes = match config.library_sha256.as_str() {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            include_str!("fixtures/compiler/original-gemm-normalized-chain18.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-gemm-normalized-chain20.json")
        }
        _ => unreachable!(),
    };
    let fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(fixture["library_sha256"], config.library_sha256);
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["cpu_count"], 16);
    assert_eq!(fixture["source_sha256"].as_object().unwrap().len(), 13);
    assert!(fixture["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12);
    let mut compared = 0;
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let source = decode(case["onnx_hex"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&source)),
            case["onnx_sha256"]
        );
        let parsed = Model::parse(&source).unwrap();
        for (name, expected) in case["initializer_sha256"].as_object().unwrap() {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(&parsed.graph.initializers[name].data)
                ),
                *expected
            );
        }
        for memory_planning in [false, true] {
            eprintln!("chain {} memory={memory_planning}", case["name"]);
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("model.onnx");
            let destination = temporary.path().join("bundle");
            fs::write(&path, &source).unwrap();
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
            );
            let compilation = match compilation {
                Ok(c) => c,
                Err(error) => {
                    assert!(!destination.exists(), "failed compile must not publish");
                    failures.push(format!(
                        "{} memory={memory_planning}: compile {error}",
                        case["name"]
                    ));
                    continue;
                }
            };
            assert_eq!(compilation.graph.backend, "cpu-llvm");
            // SAFETY: locally generated trusted immutable artifact with the
            // exact original declared input capacities, run synchronously.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            for (name, expected) in case["outputs"].as_object().unwrap() {
                let expected = decode(expected["hex"].as_str().unwrap());
                let mut actual = vec![0; model.output_size(name).unwrap()];
                model.read_output(name, &mut actual).unwrap();
                if actual != expected {
                    failures.push(format!("{} memory={memory_planning} output={name}: firstbyte={:?} actual={:x} expected={:x}", case["name"], actual.iter().zip(&expected).position(|(a,b)|a!=b), Sha256::digest(&actual), Sha256::digest(&expected)));
                }
            }
            compared += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(compared, 24);
}

#[test]
fn normalized_chain_owns_tail_and_keeps_held_and_changed_controls_fenced() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::chain, compiler_onnx::lower::LoweredGraph,
    };
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-gemm-normalized-chain18.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = chain::closures(&graph, &model).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].output, graph.names["output"]);
        assert_eq!(selected[0].terminal_output, graph.names["terminal"]);
        assert!(selected[0].normalization.is_some());
        for name in [
            "terminal",
            "norm",
            "clamped",
            "denominator",
            "h1",
            "r1",
            "add1",
            "activated1",
            "r2",
            "activated2",
        ] {
            let mut held = graph.clone();
            held.outputs
                .insert(format!("held_{name}"), held.names[name]);
            assert!(
                chain::closures(&held, &model).unwrap().is_empty(),
                "{} held={name}",
                case["name"]
            );
        }
        for name in ["minimum", "axes"] {
            let mut changed = model.clone();
            let value = changed.graph.initializers.get_mut(name).unwrap();
            value.data = if name == "minimum" {
                1u16.to_le_bytes().to_vec()
            } else {
                (-2i64).to_le_bytes().to_vec()
            };
            let lowered = LoweredGraph::lower_offline(&changed).unwrap();
            assert!(
                chain::closures(&lowered, &changed).unwrap().is_empty(),
                "{} changed={name}",
                case["name"]
            );
        }
    }
}
