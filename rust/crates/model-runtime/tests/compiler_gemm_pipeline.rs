#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, CompileOptions},
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
fn varint(mut n: usize) -> Vec<u8> {
    let mut v = vec![];
    while n > 127 {
        v.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    v.push(n as u8);
    v
}
fn field(n: usize, v: usize) -> Vec<u8> {
    [varint(n * 8), varint(v)].concat()
}
fn message(n: usize, v: &[u8]) -> Vec<u8> {
    [varint(n * 8 + 2), varint(v.len()), v.to_vec()].concat()
}
fn decode(v: &str) -> Vec<u8> {
    v.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn source(c: &Value, internal: bool) -> Vec<u8> {
    let n = c["output_channels"].as_u64().unwrap() as usize;
    let k = c["input_channels"].as_u64().unwrap() as usize;
    let tensor = |name: &str, shape: &[usize], raw: &[u8], dtype: usize| {
        message(
            5,
            &[
                shape.iter().flat_map(|v| field(1, *v)).collect::<Vec<_>>(),
                field(2, dtype),
                message(8, name.as_bytes()),
                message(9, raw),
            ]
            .concat(),
        )
    };
    let value = |tag: usize, name: &str| {
        message(
            tag,
            &[
                message(1, name.as_bytes()),
                message(
                    2,
                    &message(
                        1,
                        &[
                            field(1, 1),
                            message(
                                2,
                                &[message(1, &field(1, 1)), message(1, &field(1, n))].concat(),
                            ),
                        ]
                        .concat(),
                    ),
                ),
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
    let gemm = |a: &str, w: &str, b: &str, out: &str| {
        message(
            1,
            &[
                message(1, a.as_bytes()),
                message(1, w.as_bytes()),
                message(1, b.as_bytes()),
                message(2, out.as_bytes()),
                message(4, b"Gemm"),
                message(
                    5,
                    &[message(1, b"transB"), field(3, 1), field(20, 2)].concat(),
                ),
            ]
            .concat(),
        )
    };
    let weight = |key: &str, count: usize| {
        decode(c[key].as_str().unwrap())
            .into_iter()
            .cycle()
            .take(count * 2)
            .collect::<Vec<_>>()
    };
    let w1 = weight("weight1_pattern_hex", n * k);
    let w2 = weight("weight2_pattern_hex", k * n);
    assert_eq!(
        format!("{:x}", Sha256::digest(&w1)),
        c["weight1_sha256"].as_str().unwrap()
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&w2)),
        c["weight2_sha256"].as_str().unwrap()
    );
    let mut graph = [
        node("Mul", &["a", "scale"], "x"),
        gemm("x", "w1", "b1", "h1"),
        node("Relu", &["h1"], "h2"),
        gemm("h2", "w2", "b2", "matrix"),
    ]
    .concat();
    if internal {
        graph.extend(node("Add", &["x", "matrix"], "residual"));
        graph.extend(node("Relu", &["residual"], "activated"));
        graph.extend(node("MatMul", &["activated", "w3"], "output"));
    } else {
        graph.extend(node("Add", &["x", "matrix"], "output"));
    }
    graph.extend(tensor("scale", &[], &1.0001f32.to_le_bytes(), 1));
    graph.extend(tensor("w1", &[k, n], &w1, 10));
    graph.extend(tensor("w2", &[n, k], &w2, 10));
    if internal {
        let w3 = weight("weight3_pattern_hex", n * n);
        assert_eq!(
            format!("{:x}", Sha256::digest(&w3)),
            c["weight3_sha256"].as_str().unwrap()
        );
        graph.extend(tensor("w3", &[n, n], &w3, 10));
    }
    graph.extend(tensor(
        "b1",
        &[k],
        &decode(c["bias1_hex"].as_str().unwrap()),
        10,
    ));
    graph.extend(tensor(
        "b2",
        &[n],
        &decode(c["bias2_hex"].as_str().unwrap()),
        10,
    ));
    graph.extend(value(11, "a"));
    graph.extend(value(12, "output"));
    if c["held"].as_bool().unwrap() {
        graph.extend(value(12, "x"));
    }
    let data = [message(8, &field(2, 20)), message(7, &graph)].concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        c["onnx_sha256"].as_str().unwrap()
    );
    data
}
fn provider() -> Config {
    let library = std::path::PathBuf::from(
        std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"),
    );
    Config {
        library_sha256: format!("{:x}", Sha256::digest(fs::read(&library).unwrap())),
        library,
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
fn original_gemm_fixture_models_reconstruct_exactly() {
    for (internal, raw) in [
        include_str!("fixtures/compiler/original-gemm-residual.json"),
        include_str!("fixtures/compiler/original-gemm-internal.json"),
    ]
    .into_iter()
    .enumerate()
    {
        let fixture: Value = serde_json::from_str(raw).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let model = source(case, internal != 0);
            let model = openpilot_model_runtime::compiler_onnx::Model::parse(&model).unwrap();
            let graph =
                openpilot_model_runtime::compiler_onnx::lower::LoweredGraph::lower_offline(&model)
                    .unwrap();
            let selected = openpilot_model_runtime::compiler_backend_cpu::matrix::gemm::closures(
                &graph, &model,
            )
            .unwrap();
            assert_eq!(selected.len(), 1, "{}", case["name"]);
            assert_eq!(selected[0].relu_half, internal != 0);
            assert!(!selected[0].owned.contains(&selected[0].root));
            if internal != 0 {
                for name in ["residual", "activated"] {
                    let mut held = graph.clone();
                    held.outputs
                        .insert(format!("held_{name}"), held.names[name]);
                    assert!(
                        openpilot_model_runtime::compiler_backend_cpu::matrix::gemm::closures(
                            &held, &model
                        )
                        .unwrap()
                        .is_empty()
                    );
                }
            }
            let mut held = graph.clone();
            held.outputs.insert("held_hidden".into(), held.names["h2"]);
            assert!(
                openpilot_model_runtime::compiler_backend_cpu::matrix::gemm::closures(
                    &held, &model
                )
                .unwrap()
                .is_empty()
            );
            let mut root_reader = model.clone();
            let mut extra = root_reader
                .graph
                .nodes
                .iter()
                .find(|n| n.op == "Relu")
                .unwrap()
                .clone();
            extra.inputs = vec!["x".into()];
            extra.outputs = vec!["outside_root_reader".into()];
            root_reader.graph.nodes.push(extra);
            let mut output = root_reader.graph.outputs[0].clone();
            output.name = "outside_root_reader".into();
            root_reader.graph.outputs.push(output);
            let graph = openpilot_model_runtime::compiler_onnx::lower::LoweredGraph::lower_offline(
                &root_reader,
            )
            .unwrap();
            assert!(
                openpilot_model_runtime::compiler_backend_cpu::matrix::gemm::closures(
                    &graph,
                    &root_reader
                )
                .unwrap()
                .is_empty()
            );
            let mut shared = model.clone();
            let mut extra = shared
                .graph
                .nodes
                .iter()
                .find(|n| n.op == "Relu")
                .unwrap()
                .clone();
            extra.outputs = vec!["outside_hidden_reader".into()];
            shared.graph.nodes.push(extra);
            // A source user that is dead is pruned by scheduling; make its
            // output held so ownership must reject the shared intermediate.
            shared
                .graph
                .outputs
                .push(openpilot_model_runtime::compiler_onnx::Value {
                    name: "outside_hidden_reader".into(),
                    dtype: 1,
                    shape: vec![
                        openpilot_model_runtime::compiler_onnx::ShapeDimension::Fixed(1),
                        openpilot_model_runtime::compiler_onnx::ShapeDimension::Fixed(
                            case["input_channels"].as_i64().unwrap(),
                        ),
                    ],
                    optional: false,
                    sequence: false,
                });
            let graph =
                openpilot_model_runtime::compiler_onnx::lower::LoweredGraph::lower_offline(&shared)
                    .unwrap();
            assert!(
                openpilot_model_runtime::compiler_backend_cpu::matrix::gemm::closures(
                    &graph, &shared
                )
                .unwrap()
                .is_empty()
            );
        }
    }
}

#[test]
fn gemm_pipeline_requires_explicit_llvm_selection() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-gemm-residual.json"
    ))
    .unwrap();
    let case = &fixture["cases"][0];
    let data = source(case, false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("model.onnx");
    let dest = temp.path().join("bundle");
    fs::write(&path, data).unwrap();
    let error = compile_onnx(
        &path,
        case["onnx_sha256"].as_str().unwrap(),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &dest,
    )
    .expect_err("unproved default C path must retain its source fence");
    assert!(
        error.contains("shared view storage requires an inverse movement alias map"),
        "{error}"
    );
    assert!(!dest.exists());
}

// Each shape is a bounded compiler gate. Serialize their native compilers
// even when the Rust harness runs independent tests concurrently.
static PIPELINE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn pipeline(internal: bool, n: usize, k: usize, mode: &str) {
    let _guard = PIPELINE_GATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let fixtures = [
        include_str!("fixtures/compiler/original-gemm-residual.json"),
        include_str!("fixtures/compiler/original-gemm-internal.json"),
    ];
    let config = provider();
    {
        let fixture: Value = serde_json::from_str(fixtures[usize::from(internal)]).unwrap();
        let receipts = &fixture["provider_cases"][&config.library_sha256];
        assert!(receipts.is_object());
        let mut sources = std::collections::BTreeMap::<String, Vec<&Value>>::new();
        let cases = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["output_channels"] == n && c["input_channels"] == k && c["mode"] == mode)
            .collect::<Vec<_>>();
        assert_eq!(
            cases.len(),
            2,
            "both held-output controls must remain covered for every original mode"
        );
        for c in cases {
            sources
                .entry(c["onnx_sha256"].as_str().unwrap().into())
                .or_default()
                .push(c);
        }
        for (hash, cases) in sources {
            let data = source(cases[0], internal);
            for memory_planning in [false, true] {
                eprintln!(
                    "Gemm pipeline {} memory={memory_planning}",
                    cases[0]["name"]
                );
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("model.onnx");
                fs::write(&path, &data).unwrap();
                let dest = temp.path().join("bundle");
                let artifact = compile_onnx(
                    &path,
                    &hash,
                    &CompileOptions {
                        offline_bindings: true,
                        memory_planning,
                        llvm: Some((
                            config.clone(),
                            std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                        )),
                        ..CompileOptions::default()
                    },
                    &dest,
                )
                .unwrap();
                assert_eq!(artifact.graph.backend, "cpu-llvm");
                // SAFETY: locally compiled immutable artifact and exact declared input bytes.
                let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
                for c in &cases {
                    assert_eq!(source(c, internal), data);
                    model
                        .write_input("a", &decode(c["input_hex"].as_str().unwrap()))
                        .unwrap();
                    model.run();
                    for (name, want) in receipts[c["name"].as_str().unwrap()]["outputs"]
                        .as_object()
                        .unwrap()
                    {
                        let mut raw = vec![0; model.output_size(name).unwrap()];
                        model.read_output(name, &mut raw).unwrap();
                        let expected = decode(want["hex"].as_str().unwrap());
                        assert!(raw==expected,"{} output={name} memory={memory_planning} firstbyte={:?} native={:x} original={:x}",c["name"],raw.iter().zip(&expected).position(|(a,b)|a!=b),Sha256::digest(&raw),Sha256::digest(&expected));
                    }
                }
            }
        }
    }
}

macro_rules! pipeline_case {
    ($name:ident, $internal:expr, $n:expr, $k:expr, $mode:expr) => {
        #[test]
        fn $name() {
            pipeline($internal, $n, $k, $mode);
        }
    };
}
pipeline_case!(
    original_gemm_pipeline_final_n4k8_finite,
    false,
    4,
    8,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_final_n4k8_payload,
    false,
    4,
    8,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_final_n4k8_negativezero,
    false,
    4,
    8,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_final_n8k4_finite,
    false,
    8,
    4,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_final_n8k4_payload,
    false,
    8,
    4,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_final_n8k4_negativezero,
    false,
    8,
    4,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_final_n32k64_finite,
    false,
    32,
    64,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_final_n32k64_payload,
    false,
    32,
    64,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_final_n32k64_negativezero,
    false,
    32,
    64,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_final_n512k1024_finite,
    false,
    512,
    1024,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_final_n512k1024_payload,
    false,
    512,
    1024,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_final_n512k1024_negativezero,
    false,
    512,
    1024,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_internal_n4k8_finite,
    true,
    4,
    8,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_internal_n4k8_payload,
    true,
    4,
    8,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_internal_n4k8_negativezero,
    true,
    4,
    8,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_internal_n8k4_finite,
    true,
    8,
    4,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_internal_n8k4_payload,
    true,
    8,
    4,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_internal_n8k4_negativezero,
    true,
    8,
    4,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_internal_n32k64_finite,
    true,
    32,
    64,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_internal_n32k64_payload,
    true,
    32,
    64,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_internal_n32k64_negativezero,
    true,
    32,
    64,
    "negativezero"
);
pipeline_case!(
    original_gemm_pipeline_internal_n512k1024_finite,
    true,
    512,
    1024,
    "finite"
);
pipeline_case!(
    original_gemm_pipeline_internal_n512k1024_payload,
    true,
    512,
    1024,
    "payload"
);
pipeline_case!(
    original_gemm_pipeline_internal_n512k1024_negativezero,
    true,
    512,
    1024,
    "negativezero"
);
