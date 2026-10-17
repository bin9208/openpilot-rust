#[allow(dead_code)]
#[path = "../src/compiler_onnx/mod.rs"]
mod compiler_onnx;
use compiler_onnx::{
    lower::{IndexExpr, LoweredGraph, Plan, ScalarExpr, TensorType},
    scalar::Binary,
};
#[test]
fn independently_compiled_original_buffer_add_source_is_reproduced_from_typed_ir() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![128],
        },
        expression: ScalarExpr::Input("input".into()),
    });
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![128],
        },
        expression: ScalarExpr::Binary {
            kind: Binary::Add,
            dtype: 1,
            left: Box::new(ScalarExpr::Load {
                plan: 0,
                index: IndexExpr::Index,
            }),
            right: Box::new(ScalarExpr::FloatLiteral {
                dtype: 1,
                value: 1.,
            }),
        },
    });
    let kernel = compiler_onnx::render::render_elementwise(&graph, 1, "buffer_add").unwrap();
    assert_eq!(kernel.source, include_str!("fixtures/qcom/buffer_add.cl"));
    assert_eq!(kernel.inputs, vec![0]);
    assert_eq!(kernel.global, [128, 1, 1]);
}

#[test]
fn renderer_rejects_unimplemented_ir_and_invalid_domains_instead_of_emitting_stubs() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![1],
        },
        expression: ScalarExpr::FloatLiteral {
            dtype: 1,
            value: 2.,
        },
    });
    assert!(compiler_onnx::render::render_elementwise(&graph, 0, "bad);kernel").is_err());
    graph.plans[0].ty.shape = vec![0];
    assert!(compiler_onnx::render::render_elementwise(&graph, 0, "zero").is_err());
    graph.plans[0].ty.shape = vec![i32::MAX as usize + 1];
    assert!(compiler_onnx::render::render_elementwise(&graph, 0, "large").is_err());
    graph.plans[0].ty.shape = vec![1];
    graph.plans[0].expression = ScalarExpr::Sqrt {
        dtype: 1,
        value: Box::new(ScalarExpr::FloatLiteral {
            dtype: 1,
            value: 2.,
        }),
    };
    assert!(
        compiler_onnx::render::render_elementwise(&graph, 0, "unsupported")
            .unwrap_err()
            .contains("not implemented")
    );
}
#[test]
fn nested_arithmetic_preserves_grouping_and_repeated_load_argument_identity() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![4],
        },
        expression: ScalarExpr::Input("input".into()),
    });
    let load = ScalarExpr::Load {
        plan: 0,
        index: IndexExpr::Index,
    };
    let add = ScalarExpr::Binary {
        kind: Binary::Add,
        dtype: 1,
        left: Box::new(load.clone()),
        right: Box::new(ScalarExpr::FloatLiteral {
            dtype: 1,
            value: 1.,
        }),
    };
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![4],
        },
        expression: ScalarExpr::Binary {
            kind: Binary::Mul,
            dtype: 1,
            left: Box::new(add),
            right: Box::new(load),
        },
    });
    let kernel = compiler_onnx::render::render_elementwise(&graph, 1, "nested").unwrap();
    assert_eq!(kernel.inputs, vec![0]);
    assert!(kernel.source.contains("out[i]=(inp[i]+1.0f)*inp[i];"));
    assert_eq!(kernel.global, [4, 1, 1]);
}

#[test]
fn broadcast_index_maps_render_without_reading_outside_input_storage() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![2],
        },
        expression: ScalarExpr::Input("input".into()),
    });
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![2, 3],
        },
        expression: ScalarExpr::Load {
            plan: 0,
            index: IndexExpr::Div(Box::new(IndexExpr::Index), 3),
        },
    });
    let kernel = compiler_onnx::render::render_elementwise(&graph, 1, "broadcast").unwrap();
    assert!(kernel.source.contains("inp[(i/3)]"));
    graph.plans[1].expression = ScalarExpr::Load {
        plan: 0,
        index: IndexExpr::Index,
    };
    assert!(
        compiler_onnx::render::render_elementwise(&graph, 1, "unsafe")
            .unwrap_err()
            .contains("storage")
    );
    graph.plans[1].expression = ScalarExpr::Load {
        plan: 0,
        index: IndexExpr::Mul(Box::new(IndexExpr::Constant(0)), i128::MAX),
    };
    assert!(
        compiler_onnx::render::render_elementwise(&graph, 1, "oversized_index_factor").is_err()
    );
}

#[cfg(feature = "native-skip-miri")]
#[test]
fn compiled_elementwise_kernels_match_independent_original_output_captures() {
    use compiler_onnx::Model;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    fn decode(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect()
    }
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-lowering.json")).unwrap();
    let mut source=String::from("#include <stddef.h>\n#define __kernel\n#define __global\nstatic size_t native_index;\n#define get_global_id(axis) native_index\n");
    let mut invocations = Vec::new();
    let mut cases = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let expected = case["expected"].as_str().unwrap();
        let fields = expected.split('\t').collect::<Vec<_>>();
        if fields.len() != 5 || fields[0] != "ok" || fields[2] != "1" {
            continue;
        }
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let graph = if case["offline"].as_bool() == Some(true) {
            LoweredGraph::lower_offline(&model)
        } else {
            LoweredGraph::lower(&model)
        };
        let Ok(graph) = graph else {
            continue;
        };
        let Some(&output) = graph.outputs.get(fields[1]) else {
            continue;
        };
        let id = cases.len();
        let Ok(kernel) =
            compiler_onnx::render::render_elementwise(&graph, output, &format!("captured_{id}"))
        else {
            continue;
        };
        let mut inputs = Vec::new();
        let mut valid = true;
        for &input in &kernel.inputs {
            if graph.plans[input].ty.dtype != 1 {
                valid = false;
                break;
            }
            let bytes = match &graph.plans[input].expression {
                ScalarExpr::Constant(bytes) => bytes.clone(),
                ScalarExpr::Input(name) => {
                    let value = case["inputs"]
                        .as_array()
                        .and_then(|v| v.iter().find(|v| v["name"].as_str() == Some(name)));
                    let Some(value) = value else {
                        valid = false;
                        break;
                    };
                    decode(value["data_hex"].as_str().unwrap())
                }
                _ => {
                    valid = false;
                    break;
                }
            };
            inputs.push(
                bytes
                    .chunks_exact(4)
                    .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            );
        }
        if !valid {
            continue;
        }
        let mut args = vec!["out".to_string()];
        args.extend((0..inputs.len()).map(|i| format!("inp[{i}]")));
        invocations.push(format!(
            "case {id}: for(native_index=0;native_index<{};native_index++)captured_{id}({});break;",
            kernel.global[0],
            args.join(",")
        ));
        source.push_str(&kernel.source);
        cases.push((
            case["name"].as_str().unwrap().to_string(),
            inputs,
            decode(fields[4]),
        ));
    }
    assert!(
        cases.len() >= 20,
        "too few original cases admitted by initial renderer: {}",
        cases.len()
    );
    source.push_str(&format!(
        "void native_execute(size_t which,float*out,const float*const*inp){{switch(which){{{}}}}}",
        invocations.join("\n")
    ));
    eprintln!(
        "compiled renderer compares {} independent original cases",
        cases.len()
    );
    let directory = tempfile::tempdir().unwrap();
    let library_path = directory.path().join("rendered.so");
    let mut child = Command::new("cc")
        .args([
            "-shared",
            "-fPIC",
            "-O2",
            "-ffp-contract=off",
            "-x",
            "c",
            "-",
            "-o",
        ])
        .arg(&library_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Only the OpenCL address-space/work-item ABI is adapted for host replay;
    // every arithmetic/index expression is the actual native-rendered source.
    let library = unsafe { libloading::Library::new(&library_path) }.unwrap();
    let execute: libloading::Symbol<unsafe extern "C" fn(usize, *mut f32, *const *const f32)> =
        unsafe { library.get(b"native_execute") }.unwrap();
    for (id, (name, inputs, expected)) in cases.iter().enumerate() {
        let pointers = inputs.iter().map(|v| v.as_ptr()).collect::<Vec<_>>();
        let mut output = vec![0f32; expected.len() / 4];
        unsafe { execute(id, output.as_mut_ptr(), pointers.as_ptr()) };
        let actual = output
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(&actual, expected, "compiled original capture {name}");
    }
}

#[test]
fn preprocessing_renderer_admits_only_explicit_storage_and_arithmetic_types() {
    for (source, target) in [(2, 10), (1, 10), (10, 1)] {
        let mut graph = LoweredGraph::default();
        graph.plans.push(Plan {
            ty: TensorType {
                dtype: source,
                shape: vec![4],
            },
            expression: ScalarExpr::Input("input".into()),
        });
        graph.plans.push(Plan {
            ty: TensorType {
                dtype: target,
                shape: vec![4],
            },
            expression: ScalarExpr::Cast {
                dtype: target,
                value: Box::new(ScalarExpr::Load {
                    plan: 0,
                    index: IndexExpr::Index,
                }),
            },
        });
        let kernel = compiler_onnx::render::render_elementwise(&graph, 1, "typed_cast").unwrap();
        assert_eq!(kernel.inputs, vec![0]);
        assert!(kernel.source.contains("half"));
        graph.plans[1].ty.dtype = 16;
        assert!(
            compiler_onnx::render::render_elementwise(&graph, 1, "unsupported_bfloat").is_err()
        );
    }
}

#[test]
fn cpu_reductions_reject_unsupported_nesting_work_and_storage_domains() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![4],
        },
        expression: ScalarExpr::Input("a".into()),
    });
    let load = ScalarExpr::Load {
        plan: 0,
        index: IndexExpr::Reduction,
    };
    let reduction = |count, value| ScalarExpr::ReduceSum {
        dtype: 1,
        count,
        value: Box::new(value),
    };
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![1],
        },
        expression: reduction(0, load.clone()),
    });
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "empty").is_err());
    graph.plans[1].expression = reduction(1_048_577, load.clone());
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "large").is_err());
    graph.plans[1].expression = reduction(4, reduction(4, load.clone()));
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "nested").is_err());
    graph.plans[1].expression = reduction(
        4,
        ScalarExpr::Binary {
            kind: Binary::Mul,
            dtype: 1,
            left: Box::new(load.clone()),
            right: Box::new(load),
        },
    );
    assert!(
        compiler_onnx::render::render_cpu_elementwise(&graph, 1, "unprofiled_product").is_err()
    );
    let image_load = ScalarExpr::StorageRound {
        dtype: 10,
        value: Box::new(ScalarExpr::Load {
            plan: 0,
            index: IndexExpr::Reduction,
        }),
    };
    graph.plans[1].expression = reduction(
        4,
        ScalarExpr::Binary {
            kind: Binary::Mul,
            dtype: 1,
            left: Box::new(image_load.clone()),
            right: Box::new(image_load),
        },
    );
    let matrix =
        compiler_onnx::render::render_cpu_elementwise(&graph, 1, "profiled_image_dot").unwrap();
    assert!(matrix.source.contains("__builtin_fmaf"));
    // Matrix admission requires both observable half-storage boundaries.
    if let ScalarExpr::ReduceSum { value, .. } = &mut graph.plans[1].expression {
        if let ScalarExpr::Binary { right, .. } = value.as_mut() {
            **right = ScalarExpr::Load {
                plan: 0,
                index: IndexExpr::Reduction,
            };
        }
    }
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "mixed_image_dot").is_err());
    graph.plans[1].expression = ScalarExpr::PaddedLoad {
        plan: 0,
        coordinates: vec![(IndexExpr::Index, 0, 5)],
    };
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "invalid_crop").is_err());
    graph.plans[1].expression = ScalarExpr::PaddedLoad {
        plan: 0,
        coordinates: vec![(IndexExpr::Constant(i64::MAX as i128 + 1), 0, 4)],
    };
    assert!(compiler_onnx::render::render_cpu_elementwise(&graph, 1, "large_index").is_err());
}
