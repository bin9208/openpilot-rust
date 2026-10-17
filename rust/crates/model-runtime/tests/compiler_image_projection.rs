#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::image::{residual_closures, ResidualStorage},
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    compiler_onnx::{lower::LoweredGraph, Model},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_residual_closure_keeps_held_and_external_storage() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-storage-alias.json"
    ))
    .unwrap();
    let mut selected = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if !case["name"].as_str().unwrap().starts_with('c') {
            continue;
        }
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let model = Model::parse(&bytes).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let closures = residual_closures(&graph, &model).unwrap();
        let conv = model.graph.nodes.iter().find(|n| n.op == "Conv").unwrap();
        let input = graph.names[&conv.inputs[0]];
        let channels = graph.plans[input].ty.shape[1];
        if !channels.is_multiple_of(4) {
            assert!(closures.is_empty());
            continue;
        }
        assert_eq!(closures.len(), 1, "{}", case["name"]);
        let closure = &closures[0];
        selected += 1;
        assert_eq!(closure.input, input);
        assert_eq!(closure.weights, graph.names[&conv.inputs[1]]);
        assert_eq!(closure.image.channels_in, channels);
        assert_eq!(closure.image.width, graph.plans[input].ty.shape[3]);
        assert!(!closure.retain_convolution);
        let computed = model.graph.nodes.iter().any(|n| n.op == "Mul");
        let held = model.graph.outputs.iter().any(|o| o.name == conv.inputs[0]);
        assert_eq!(closure.retain_input, held);
        assert_eq!(
            closure.residual_storage,
            if computed {
                ResidualStorage::PackedHalf
            } else {
                ResidualStorage::LogicalF32
            },
            "{}",
            case["name"]
        );
    }
    assert_eq!(selected, 24);
}

#[test]
fn full_onnx_image_closure_preserves_looped_zero_identity() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-image-logical-artifact.json"
    ))
    .unwrap();
    let library = std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required");
    let pin = format!("{:x}", Sha256::digest(std::fs::read(library).unwrap()));
    let provider = fixture["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["library_sha256"] == pin)
        .unwrap();
    let case = provider["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "control-20-1-negativezero")
        .unwrap();
    let source = decode(case["onnx_hex"].as_str().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("model.onnx");
    std::fs::write(&input, &source).unwrap();
    let bundle = dir.path().join("bundle");
    compile_onnx(
        &input,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &bundle,
    )
    .unwrap();
    // SAFETY: the native compiler owns and verifies this exact-source artifact;
    // the private directory is immutable while its synchronous model is loaded.
    let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
    model
        .write_input("a", &decode(case["raw_input_hex"].as_str().unwrap()))
        .unwrap();
    model.run();
    let mut output = vec![0; model.output_size("output").unwrap()];
    model.read_output("output", &mut output).unwrap();
    assert_eq!(output, decode(case["output_hex"].as_str().unwrap()));
}

fn provider() -> (
    Value,
    openpilot_model_runtime::compiler_backend_cpu::llvm::Config,
) {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-image-logical-artifact.json"
    ))
    .unwrap();
    let library = std::path::PathBuf::from(
        std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"),
    );
    let pin = format!("{:x}", Sha256::digest(std::fs::read(&library).unwrap()));
    let provider = fixture["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["library_sha256"] == pin)
        .unwrap();
    let config = openpilot_model_runtime::compiler_backend_cpu::llvm::Config {
        library,
        library_sha256: pin,
        triple: provider["triple"].as_str().unwrap().into(),
        cpu: provider["cpu"].as_str().unwrap().into(),
        features: provider["features"].as_str().unwrap().into(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    (provider.clone(), config)
}

#[test]
fn actual_compiler_uses_typed_llvm_closure_with_both_memory_plans() {
    let (provider, config) = provider();
    for memory_planning in [false, true] {
        for case in provider["cases"].as_array().unwrap() {
            eprintln!("IMAGE graph {} memory={memory_planning}", case["name"]);
            let source = decode(case["onnx_hex"].as_str().unwrap());
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("model.onnx");
            std::fs::write(&input, &source).unwrap();
            let bundle = dir.path().join("bundle");
            let compilation = compile_onnx(
                &input,
                &format!("{:x}", Sha256::digest(&source)),
                &CompileOptions {
                    offline_bindings: true,
                    memory_planning,
                    llvm: Some((
                        config.clone(),
                        std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                    )),
                    ..CompileOptions::default()
                },
                &bundle,
            )
            .unwrap_or_else(|e| panic!("{}:{e}", case["name"]));
            assert_eq!(compilation.graph.backend, "cpu-llvm");
            assert_eq!(compilation.graph.calls.len(), 2);
            // SAFETY: the compiler owns these pinned sources and verified synchronous
            // wrappers; this private artifact stays immutable while its model lives.
            let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
            model
                .write_input("a", &decode(case["raw_input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            let mut output = vec![0; model.output_size("output").unwrap()];
            model.read_output("output", &mut output).unwrap();
            assert_eq!(
                output,
                decode(case["output_hex"].as_str().unwrap()),
                "{} memory={memory_planning}",
                case["name"]
            );
        }
    }
}

#[test]
fn integrated_llvm_preserves_original_held_outputs() {
    let (_, config) = provider();
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-storage-alias.json"
    ))
    .unwrap();
    let mut selected = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if !case["name"].as_str().unwrap().starts_with('c') {
            continue;
        }
        let source = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = Model::parse(&source).unwrap();
        let lowered = LoweredGraph::lower_offline(&onnx).unwrap();
        let closures = residual_closures(&lowered, &onnx).unwrap();
        if !closures
            .iter()
            .any(|c| c.residual_storage == ResidualStorage::PackedHalf)
        {
            continue;
        }
        selected += 1;
        eprintln!("IMAGE held graph {}", case["name"]);
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("model.onnx");
        std::fs::write(&input, &source).unwrap();
        let bundle = dir.path().join("bundle");
        let compilation = compile_onnx(
            &input,
            &format!("{:x}", Sha256::digest(&source)),
            &CompileOptions {
                offline_bindings: true,
                llvm: Some((
                    config.clone(),
                    std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                )),
                ..CompileOptions::default()
            },
            &bundle,
        )
        .unwrap_or_else(|e| panic!("{}:{e}", case["name"]));
        assert_eq!(compilation.graph.backend, "cpu-llvm");
        // SAFETY: this immutable private artifact was emitted by the native
        // compiler from verified sources with checked synchronous ABI views.
        let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
        model.run();
        for output in &compilation.graph.outputs {
            let mut bytes = vec![0; model.output_size(&output.name).unwrap()];
            model.read_output(&output.name, &mut bytes).unwrap();
            assert_eq!(
                bytes,
                decode(case["outputs"][&output.name].as_str().unwrap()),
                "{}:{}",
                case["name"],
                output.name
            );
        }
    }
    assert_eq!(selected, 12);
}

#[test]
fn selected_llvm_provider_failure_never_publishes_or_falls_back() {
    let (provider, mut config) = provider();
    let source = decode(provider["cases"][0]["onnx_hex"].as_str().unwrap());
    config.library_sha256 = "0".repeat(64);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("model.onnx");
    std::fs::write(&input, &source).unwrap();
    let bundle = dir.path().join("bundle");
    let result = compile_onnx(
        &input,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            llvm: Some((
                config,
                std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &bundle,
    );
    assert!(result.unwrap_err().contains("SHA256 mismatch"));
    assert!(!bundle.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
