#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    compiler_onnx::{
        lower::{IndexExpr, LoweredGraph, Plan, ScalarExpr, TensorType},
        render,
    },
    CpuModel,
};
use sha2::{Digest, Sha256};
use std::fs;
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "fixtures/compiler/original-conditional-cpu.json"
    ))
    .unwrap()
}
fn run(case: &serde_json::Value, memory: bool) {
    let temp = tempfile::tempdir().unwrap();
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let path = temp.path().join("model.onnx");
    fs::write(&path, &bytes).unwrap();
    if let Some(outputs) = case["outputs"].as_object() {
        let parsed = openpilot_model_runtime::compiler_onnx::Model::parse(&bytes).unwrap();
        let lowered = if case["offline_bindings"].as_bool().unwrap_or(true) {
            LoweredGraph::lower_offline(&parsed)
        } else {
            LoweredGraph::lower(&parsed)
        }
        .unwrap();
        for (name, expected) in outputs {
            if let Some(shape) = expected["shape"].as_array() {
                let actual = &lowered.plans[lowered.outputs[name]].ty;
                assert_eq!(
                    actual.shape,
                    shape
                        .iter()
                        .map(|n| n.as_u64().unwrap() as usize)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    actual.dtype,
                    if expected["dtype"] == "dtypes.half" {
                        10
                    } else {
                        1
                    }
                );
            }
        }
    }
    let dest = temp.path().join("bundle");
    compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: case["offline_bindings"].as_bool().unwrap_or(true),
            memory_planning: memory,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    if let Some(inputs) = case["inputs"].as_object() {
        for (n, v) in inputs {
            model.write_input(n, &decode(v.as_str().unwrap())).unwrap();
        }
    } else {
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
    }
    model.run();
    if let Some(outputs) = case["outputs"].as_object() {
        for (n, v) in outputs {
            let mut actual = vec![0; model.output_size(n).unwrap()];
            model.read_output(n, &mut actual).unwrap();
            assert_eq!(
                actual,
                decode(v["hex"].as_str().unwrap()),
                "{} {n}",
                case["name"]
            );
        }
    } else {
        let mut actual = vec![0; model.output_size("output").unwrap()];
        model.read_output("output", &mut actual).unwrap();
        assert_eq!(
            actual,
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn original_float_concat_payloads_axes_held_outputs_and_nonuniform_branches_match() {
    for case in fixture()["cases"].as_array().unwrap() {
        for memory in [false, true] {
            run(case, memory);
        }
    }
}
#[test]
fn original_relu_and_matrix_concat_padding_barriers_match() {
    for case in fixture()["matrix_barrier_cases"].as_array().unwrap() {
        if !case["name"].as_str().unwrap().starts_with("Add-") {
            for memory in [false, true] {
                run(case, memory);
            }
        }
    }
}
#[test]
fn direct_matrix_arithmetic_aliases_remain_rejected_before_publication() {
    for case in fixture()["matrix_barrier_cases"].as_array().unwrap() {
        if !case["name"].as_str().unwrap().starts_with("Add-") {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        assert!(compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest
        )
        .is_err());
        assert!(!dest.exists());
    }
}
fn branch_graph() -> LoweredGraph {
    let mut g = LoweredGraph::default();
    for (n, count) in [("a", 2), ("b", 3)] {
        g.plans.push(Plan {
            ty: TensorType {
                dtype: 1,
                shape: vec![count],
            },
            expression: ScalarExpr::Input(n.into()),
        });
    }
    g.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![5],
        },
        expression: ScalarExpr::Select {
            condition: Box::new(ScalarExpr::IndexLess {
                index: IndexExpr::Index,
                bound: 2,
            }),
            yes: Box::new(ScalarExpr::Load {
                plan: 0,
                index: IndexExpr::Index,
            }),
            no: Box::new(ScalarExpr::Load {
                plan: 1,
                index: IndexExpr::Add(
                    Box::new(IndexExpr::Index),
                    Box::new(IndexExpr::Constant(-2)),
                ),
            }),
        },
    });
    g
}
#[test]
fn branch_domains_prove_each_load_without_disabling_bounds_checks() {
    let mut g = branch_graph();
    assert!(render::render_cpu_elementwise(&g, 2, "bounded_branches").is_ok());
    assert!(render::render_elementwise(&g, 2, "gpu_conditional").is_err());
    if let ScalarExpr::Select { yes, .. } = &mut g.plans[2].expression {
        **yes = ScalarExpr::Load {
            plan: 0,
            index: IndexExpr::Add(
                Box::new(IndexExpr::Index),
                Box::new(IndexExpr::Constant(-1)),
            ),
        };
    }
    assert!(render::render_cpu_elementwise(&g, 2, "negative_index").is_err());
    let mut g = branch_graph();
    if let ScalarExpr::Select { no, .. } = &mut g.plans[2].expression {
        **no = ScalarExpr::Load {
            plan: 1,
            index: IndexExpr::Index,
        };
    }
    assert!(render::render_cpu_elementwise(&g, 2, "high_index").is_err());
}

fn vi(data: &[u8], pos: &mut usize) -> usize {
    let mut n = 0;
    let mut shift = 0;
    loop {
        let b = data[*pos];
        *pos += 1;
        n |= ((b & 127) as usize) << shift;
        if b < 128 {
            return n;
        }
        shift += 7;
        assert!(shift < 64);
    }
}
fn fields(data: &[u8]) -> Vec<(usize, &[u8], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let start = pos;
        let tag = vi(data, &mut pos);
        let payload = match tag & 7 {
            2 => {
                let len = vi(data, &mut pos);
                let p = &data[pos..pos + len];
                pos += len;
                p
            }
            0 => {
                vi(data, &mut pos);
                &data[pos..pos]
            }
            1 => {
                pos += 8;
                &data[pos..pos]
            }
            5 => {
                pos += 4;
                &data[pos..pos]
            }
            _ => panic!("unsupported trusted fixture wire"),
        };
        out.push((tag >> 3, payload, &data[start..pos]));
    }
    out
}
fn varint(mut v: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while v > 127 {
        out.push((v as u8 & 127) | 128);
        v >>= 7;
    }
    out.push(v as u8);
    out
}
fn message(n: usize, data: &[u8]) -> Vec<u8> {
    [varint(n * 8 + 2), varint(data.len()), data.to_vec()].concat()
}
fn actual_prefix_source(case: &serde_json::Value) -> Vec<u8> {
    let directory = option_env!("CARGO_MANIFEST_DIR").unwrap_or("rust/crates/model-runtime");
    let path = std::path::Path::new(directory)
        .join("../../../openpilot/selfdrive/modeld/models")
        .join(case["source_model"].as_str().unwrap());
    let source = fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        case["source_model_sha256"].as_str().unwrap()
    );
    let top = fields(&source);
    let graph = fields(top.iter().find(|v| v.0 == 7).unwrap().1);
    let nodes = graph
        .iter()
        .filter(|v| v.0 == 1)
        .take(case["last_node"].as_u64().unwrap() as usize + 1)
        .collect::<Vec<_>>();
    let required = nodes
        .iter()
        .flat_map(|v| fields(v.1))
        .filter(|v| v.0 == 1)
        .map(|v| v.1)
        .collect::<std::collections::BTreeSet<_>>();
    let mut prefix = nodes
        .iter()
        .flat_map(|v| v.2.iter().copied())
        .collect::<Vec<_>>();
    for (n, payload, raw) in &graph {
        if *n == 1 || *n == 12 {
            continue;
        }
        if *n == 5 {
            let name = fields(payload).into_iter().find(|v| v.0 == 8).unwrap().1;
            if !required.contains(name) {
                continue;
            }
        }
        prefix.extend_from_slice(raw);
    }
    let mut output = message(1, case["output_name"].as_str().unwrap().as_bytes());
    output.extend(message(2, &message(1, &[8, 1])));
    prefix.extend(message(12, &output));
    let bytes = top
        .iter()
        .flat_map(|(n, _, raw)| {
            if *n == 7 {
                message(7, &prefix)
            } else {
                raw.to_vec()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        case["prefix_sha256"].as_str().unwrap()
    );
    bytes
}

fn actual_prefix_output(c: &serde_json::Value) -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let bytes = if c["whole_model"].as_bool() == Some(true) {
        let directory = option_env!("CARGO_MANIFEST_DIR").unwrap_or("rust/crates/model-runtime");
        let bytes = fs::read(
            std::path::Path::new(directory)
                .join("../../../openpilot/selfdrive/modeld/models")
                .join(c["source_model"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            c["source_model_sha256"].as_str().unwrap()
        );
        bytes
    } else {
        actual_prefix_source(c)
    };
    if let Some(shape) = c["original"]["shape"].as_array() {
        let parsed = openpilot_model_runtime::compiler_onnx::Model::parse(&bytes).unwrap();
        let lowered = LoweredGraph::lower_offline(&parsed).unwrap();
        let name = c["output_name"].as_str().unwrap();
        assert_eq!(lowered.plans[lowered.outputs[name]].ty.dtype, 1);
        assert_eq!(
            lowered.plans[lowered.outputs[name]].ty.shape,
            shape
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<Vec<_>>()
        );
    }
    let path = temp.path().join("model.onnx");
    fs::write(&path, &bytes).unwrap();
    let dest = temp.path().join("bundle");
    let compilation = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    for input in &compilation.graph.inputs {
        let bytes = match input.name.as_str() {
            "img" => (0u8..=255).collect::<Vec<_>>().repeat(1536),
            "big_img" => (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
            "desire_pulse" => vec![0; 25 * 8 * 4],
            "traffic_convention" | "action_t" => vec![0; 2 * 4],
            "features_buffer" => vec![0; 24 * 512 * 4],
            "input_img" => (0u8..=255).collect::<Vec<_>>().repeat(5400),
            "calib" => [1.0f32 + 2f32.powi(-11), -0.0, 0.3]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
            _ => panic!("unknown input"),
        };
        model.write_input(&input.name, &bytes).unwrap();
    }
    model.run();
    let name = c["output_name"].as_str().unwrap();
    let mut actual = vec![0; model.output_size(name).unwrap()];
    model.read_output(name, &mut actual).unwrap();
    actual
}
#[test]
fn actual_driver_true_prefix_relu_matches_independent_original_export() {
    let f = fixture();
    let c = &f["actual_prefix100"];
    let actual = actual_prefix_output(c);
    assert_eq!(
        actual.len() as u64,
        c["original"]["bytes"].as_u64().unwrap()
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&actual)),
        c["original"]["sha256"].as_str().unwrap()
    );
}
#[test]
fn actual_mixed_concat_prefix_matches_original_true_float_export() {
    let f = fixture();
    let c = &f["actual_prefix98"];
    let actual = actual_prefix_output(c);
    assert_eq!(
        actual.len() as u64,
        c["original"]["bytes"].as_u64().unwrap()
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&actual)),
        c["original"]["sha256"].as_str().unwrap()
    );
}
#[test]
fn original_mixed_concat_preserves_virtual_and_physical_half_storage_boundaries() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-mixed-conditional-cpu.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        for memory in [false, true] {
            run(case, memory);
        }
    }
}

#[test]
fn unproved_half_matrix_operand_aliases_stay_rejected_before_publication() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-mixed-conditional-cpu.json"
    ))
    .unwrap();
    for case in f["unproved_matrix_aliases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(result
            .err()
            .unwrap()
            .contains("mixed half Concat operand alias"));
        assert!(!dest.exists());
    }
}

#[test]
fn actual_compiled_driver_whole_export_matches_original_every_byte() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-mixed-conditional-cpu.json"
    ))
    .unwrap();
    let c = &f["actual_driver_whole_model"];
    let actual = actual_prefix_output(c);
    assert_eq!(c["original"]["shape"], serde_json::json!([1, 553]));
    assert_eq!(actual, decode(c["original"]["hex"].as_str().unwrap()));
}
fn fusion_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "fixtures/compiler/original-fusion-boundary.json"
    ))
    .unwrap()
}
fn fusion_source(case: &serde_json::Value) -> Vec<u8> {
    let base = actual_prefix_source(&case["base"]);
    let top = fields(&base);
    let graph = fields(top.iter().find(|v| v.0 == 7).unwrap().1);
    let mut body = Vec::new();
    for (n, payload, raw) in &graph {
        if *n == 12 {
            continue;
        }
        let rewrite = &case["node_rewrite"];
        if *n == 1
            && !rewrite.is_null()
            && fields(payload).iter().any(|(n, v, _)| {
                *n == 2 && *v == rewrite["before_output"].as_str().unwrap().as_bytes()
            })
        {
            body.extend(decode(rewrite["insert_node_hex"].as_str().unwrap()));
            let updated = fields(payload)
                .iter()
                .flat_map(|(n, v, raw)| {
                    if *n == 1 && *v == rewrite["old_input"].as_str().unwrap().as_bytes() {
                        message(1, rewrite["new_input"].as_str().unwrap().as_bytes())
                    } else {
                        raw.to_vec()
                    }
                })
                .collect::<Vec<_>>();
            body.extend(message(1, &updated));
        } else {
            body.extend_from_slice(raw);
        }
    }
    body.extend(decode(case["append_graph_hex"].as_str().unwrap()));
    let source = top
        .iter()
        .flat_map(|(n, _, raw)| {
            if *n == 7 {
                message(7, &body)
            } else {
                raw.to_vec()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        case["derived_sha256"].as_str().unwrap()
    );
    source
}
#[test]
fn actual_fused_prefix_matches_independent_original_final_output() {
    use openpilot_model_runtime::compiler_backend_cpu::llvm::Config;
    let fixture = fusion_fixture();
    let case = fixture["admission_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "direct")
        .unwrap();
    let source = fusion_source(case);
    let library = std::path::PathBuf::from(std::env::var_os("LLVM_PATH").unwrap());
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    assert_eq!(
        library_sha256, "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0",
        "prefix93 original comparison requires sealed LLVM18"
    );
    let config = Config {
        library,
        library_sha256,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "znver3".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("prefix.onnx");
    fs::write(&path, &source).unwrap();
    let destination = temp.path().join("bundle");
    let compilation = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            llvm: Some((
                config,
                std::path::PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &destination,
    )
    .unwrap();
    assert_eq!(compilation.graph.backend, "cpu-llvm");
    // Reconstruct only public raw model inputs; no captured Conv/Gelu/Mean
    // intermediate is read or written into this cold native candidate.
    let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
    for input in &compilation.graph.inputs {
        let bytes = match input.name.as_str() {
            "img" => (0u8..=255).collect::<Vec<_>>().repeat(1536),
            "big_img" => (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
            "desire_pulse" => vec![0; 25 * 8 * 4],
            "traffic_convention" | "action_t" => vec![0; 2 * 4],
            "features_buffer" => vec![0; 24 * 512 * 4],
            "input_img" => (0u8..=255).collect::<Vec<_>>().repeat(5400),
            "calib" => [1.0f32 + 2f32.powi(-11), -0.0, 0.3]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
            _ => panic!("unknown input"),
        };
        model.write_input(&input.name, &bytes).unwrap();
    }
    model.run();
    let expected = &case["outputs"]["mean"];
    assert_eq!(expected["sha256"], fixture["true_prefix"]["sha256"]);
    let mut actual = vec![0; model.output_size("mean").unwrap()];
    model.read_output("mean", &mut actual).unwrap();
    assert_eq!(actual.len() as u64, expected["bytes"].as_u64().unwrap());
    assert_eq!(actual, decode(expected["hex"].as_str().unwrap()));
}
#[test]
fn unproved_observable_fusion_rejects_direct_views_arithmetic_and_multiple_outputs() {
    for case in fusion_fixture()["admission_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["reject"] == true)
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = fusion_source(case);
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(
            result.is_err(),
            "{} published unproved fusion",
            case["name"]
        );
        assert!(result
            .err()
            .unwrap()
            .contains("observable Conv/Gelu/ReduceMean fusion"));
        assert!(!dest.exists());
    }
}
#[test]
fn verified_materialization_dead_branch_and_quantized_fusion_controls_match_original() {
    for case in fusion_fixture()["admission_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["reject"] == false)
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = fusion_source(case);
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let compilation = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        )
        .unwrap_or_else(|e| panic!("{} {e}", case["name"]));
        let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
        for input in &compilation.graph.inputs {
            let bytes = match input.name.as_str() {
                "img" => (0u8..=255).collect::<Vec<_>>().repeat(1536),
                "big_img" => (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
                "desire_pulse" => vec![0; 25 * 8 * 4],
                "traffic_convention" | "action_t" => vec![0; 2 * 4],
                "features_buffer" => vec![0; 24 * 512 * 4],
                "input_img" => (0u8..=255).collect::<Vec<_>>().repeat(5400),
                "calib" => [1.0f32 + 2f32.powi(-11), -0.0, 0.3]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
                _ => panic!("unknown input"),
            };
            model.write_input(&input.name, &bytes).unwrap();
        }
        model.run();
        for (name, expected) in case["outputs"].as_object().unwrap() {
            let mut bytes = vec![0; model.output_size(name).unwrap()];
            model.read_output(name, &mut bytes).unwrap();
            assert_eq!(bytes.len() as u64, expected["bytes"].as_u64().unwrap());
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                expected["sha256"].as_str().unwrap(),
                "{} {name}",
                case["name"]
            );
            if let Some(hex) = expected["hex"].as_str() {
                assert_eq!(bytes, decode(hex));
            }
        }
    }
}
#[test]
fn mean_on_independently_materialized_original_gelu_remains_exact() {
    let case = &fusion_fixture()["materialized_positive"];
    for memory in [false, true] {
        run(case, memory);
    }
}

#[test]
fn original_all_half_concat_physical_virtual_held_and_export_contexts_match() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-half-concat-cpu.json"
    ))
    .unwrap();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["compile_supported"] == true)
    {
        for memory in [false, true] {
            run(case, memory);
        }
    }
}

#[test]
fn ambiguous_original_valid_half_concat_contexts_reject_without_publication() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-half-concat-cpu.json"
    ))
    .unwrap();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["compile_supported"] == false)
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: case["offline_bindings"].as_bool().unwrap(),
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(
            result
                .err()
                .is_some_and(|error| error.contains("physical-half Concat movement")),
            "{}",
            case["name"]
        );
        assert!(!dest.exists(), "{}", case["name"]);
    }
}

#[test]
fn actual_driving_true_final_half_concat_normalization_and_first_conv_match() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-half-concat-cpu.json"
    ))
    .unwrap();
    for case in fixture["driving_preprocess"]["cases"].as_array().unwrap() {
        let actual = actual_prefix_output(case);
        assert_eq!(
            actual.len() as u64,
            case["original"]["bytes"].as_u64().unwrap()
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&actual)),
            case["original"]["sha256"].as_str().unwrap(),
            "{}",
            case["last_node"]
        );
    }
}

#[test]
fn original_vector_initializer_division_folds_before_late_fdiv_rewrite() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-half-concat-cpu.json"
    ))
    .unwrap();
    for case in fixture["vector_division"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            let name = case["name"].as_str().unwrap();
            name.ends_with("-uniform")
                || name.ends_with("-normal")
                || name.starts_with("vectorDiv-1-") && name.ends_with("-small")
        })
        .chain(fixture["vector_dynamic_positive"].as_array().unwrap())
    {
        for memory in [false, true] {
            run(case, memory);
        }
    }
}

#[test]
fn unproved_vector_initializer_special_reciprocals_reject_without_publication() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-half-concat-cpu.json"
    ))
    .unwrap();
    for case in fixture["vector_division"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            let name = case["name"].as_str().unwrap();
            !(name.ends_with("-uniform")
                || name.ends_with("-normal")
                || name.starts_with("vectorDiv-1-") && name.ends_with("-small"))
        })
        .chain(fixture["vector_view_rejections"].as_array().unwrap())
    {
        let temp = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &bytes).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(
            result.err().is_some_and(|error| error.contains(
                case["expected_error"]
                    .as_str()
                    .unwrap_or("constant-vector reciprocal")
            )),
            "{}",
            case["name"]
        );
        assert!(!dest.exists());
    }
}
