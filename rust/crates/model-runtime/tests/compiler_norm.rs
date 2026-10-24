#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_onnx::{
    lower::{IndexExpr, LoweredGraph, Plan, ScalarExpr, TensorType},
    render,
};
use std::{fs, process::Command};
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/compiler/original-norm.json")).unwrap()
}
fn assert_same_bits(actual: &[u8], expected: &[u8], name: &str) {
    assert_eq!(actual.len(), expected.len(), "{name} length");
    let differences = actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| {
            (
                i,
                format!("{:08x}", u32::from_le_bytes(a.try_into().unwrap())),
                format!("{:08x}", u32::from_le_bytes(b.try_into().unwrap())),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "{name}: {} differing float32 patterns; first {:?}",
        differences.len(),
        &differences[..differences.len().min(8)]
    );
}
#[test]
fn native_sqrt_matches_original_dynamic_float32_bits() {
    let f = fixture();
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] == "Sqrt")
    {
        let inputs = [decode(case["input_hex"].as_str().unwrap())];
        let count = inputs[0].len() / 4;
        let mut graph = LoweredGraph::default();
        for i in 0..inputs.len() {
            graph.plans.push(Plan {
                ty: TensorType {
                    dtype: 1,
                    shape: vec![count],
                },
                expression: ScalarExpr::Input(format!("input{i}")),
            });
        }
        let load = |plan| {
            Box::new(ScalarExpr::Load {
                plan,
                index: IndexExpr::Index,
            })
        };
        let expression = ScalarExpr::Sqrt {
            dtype: 1,
            value: load(0),
        };
        graph.plans.push(Plan {
            ty: TensorType {
                dtype: 1,
                shape: vec![count],
            },
            expression,
        });
        let kernel = render::render_cpu_elementwise(&graph, inputs.len(), "math_probe").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("probe.c");
        let library = directory.path().join("probe.so");
        let mut args = vec!["(float*)buffers[0]".to_string()];
        args.extend(
            kernel
                .inputs
                .iter()
                .map(|v| format!("(const float*)buffers[{}]", v + 1)),
        );
        fs::write(&source,format!("#include <stddef.h>\n#define __kernel\n#define __global\nstatic size_t native_index;\n#define get_global_id(axis) native_index\n{}\nvoid native_execute(void **buffers){{for(native_index=0;native_index<{count};native_index++)math_probe({});}}",kernel.source,args.join(","))).unwrap();
        let compiled = Command::new("clang")
            .args([
                "-shared",
                "-fPIC",
                "-O2",
                "-fno-fast-math",
                "-ffp-contract=off",
            ])
            .arg(&source)
            .args(["-lm", "-o"])
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let library = unsafe { libloading::Library::new(&library) }.unwrap();
        let execute: libloading::Symbol<unsafe extern "C" fn(*mut *mut std::ffi::c_void)> =
            unsafe { library.get(b"native_execute") }.unwrap();
        let mut output = vec![0u32; count];
        let mut buffers = vec![output.as_mut_ptr().cast()];
        buffers.extend(inputs.iter().map(|v| v.as_ptr().cast_mut().cast()));
        unsafe { execute(buffers.as_mut_ptr()) };
        let actual = output
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        assert_same_bits(
            &actual,
            &decode(case["expected_hex"].as_str().unwrap()),
            case["name"].as_str().unwrap(),
        );
    }
}
#[test]
fn native_square_reductions_match_original_schedules() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::{compile_onnx, CompileOptions},
        CpuModel,
    };
    use sha2::{Digest, Sha256};
    for memory_planning in [false, true] {
        for case in fixture()["cases"].as_array().unwrap().iter().filter(|c| {
            !c["onnx_hex"].is_null() && c["admission"].is_null() && c["norm_admission"].is_null()
        }) {
            let directory = tempfile::tempdir().unwrap();
            let bytes = decode(case["onnx_hex"].as_str().unwrap());
            let onnx = directory.path().join("norm.onnx");
            fs::write(&onnx, &bytes).unwrap();
            let bundle = directory.path().join("bundle");
            compile_onnx(
                &onnx,
                &format!("{:x}", Sha256::digest(&bytes)),
                &CompileOptions {
                    memory_planning,
                    ..CompileOptions::default()
                },
                &bundle,
            )
            .unwrap();
            let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            let mut output = vec![0; model.output_size("output").unwrap()];
            model.read_output("output", &mut output).unwrap();
            assert_same_bits(
                &output,
                &decode(case["expected_hex"].as_str().unwrap()),
                case["name"].as_str().unwrap(),
            );
            if let Some(outputs) = case["joint_outputs"].as_object() {
                for (name, expected) in outputs {
                    let mut output = vec![0; model.output_size(name).unwrap()];
                    model.read_output(name, &mut output).unwrap();
                    assert_same_bits(
                        &output,
                        &decode(expected.as_str().unwrap()),
                        &format!("{}:{name}", case["name"]),
                    );
                }
            }
        }
    }
}

#[test]
fn computed_norm_inputs_reject_even_when_jointly_exported() {
    use openpilot_model_runtime::compiler_backend_cpu::{compile_onnx, CompileOptions};
    use sha2::{Digest, Sha256};
    for case in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["admission"].is_null())
    {
        assert_eq!(case["admission"], "pending");
        let directory = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = directory.path().join("norm.onnx");
        fs::write(&onnx, &bytes).unwrap();
        let bundle = directory.path().join("bundle");
        let error = compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &bundle,
        )
        .unwrap_err();
        assert!(
            error.contains("CPU sqrt") || error.contains("product reduction scheduling"),
            "{}: {error}",
            case["name"]
        );
        assert!(!bundle.exists());
    }
}
#[test]
fn normalization_admission_rejects_unproved_axis_and_value_maps() {
    use openpilot_model_runtime::compiler_backend_cpu::{compile_onnx, CompileOptions};
    use sha2::{Digest, Sha256};
    for case in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["norm_admission"] == "reject")
    {
        let directory = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = directory.path().join("norm.onnx");
        fs::write(&onnx, &bytes).unwrap();
        let bundle = directory.path().join("bundle");
        let error = compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions::default(),
            &bundle,
        )
        .unwrap_err();
        assert!(
            error.contains("reciprocal computed context")
                || error.contains("reciprocal broadcast-bearing view context")
                || error.contains("CPU sqrt")
                || error.contains("product reduction scheduling"),
            "{}: {error}",
            case["name"]
        );
        assert!(!bundle.exists());
    }
}
#[test]
fn computed_sqrt_ancestry_is_not_an_admitted_primitive() {
    use openpilot_model_runtime::compiler_onnx::scalar::Binary;
    let load = || {
        Box::new(ScalarExpr::Load {
            plan: 0,
            index: IndexExpr::Index,
        })
    };
    let product = || ScalarExpr::Binary {
        kind: Binary::Mul,
        dtype: 1,
        left: load(),
        right: load(),
    };
    for held in [false, true] {
        for value in [
            product(),
            ScalarExpr::Binary {
                kind: Binary::Add,
                dtype: 1,
                left: load(),
                right: Box::new(ScalarExpr::FloatLiteral {
                    dtype: 1,
                    value: 1.0,
                }),
            },
            ScalarExpr::Exp2 {
                dtype: 1,
                value: load(),
            },
            ScalarExpr::Load {
                plan: 1,
                index: IndexExpr::Index,
            },
        ] {
            let mut graph = LoweredGraph::default();
            let ty = TensorType {
                dtype: 1,
                shape: vec![4],
            };
            graph.plans.push(Plan {
                ty: ty.clone(),
                expression: ScalarExpr::Input("a".into()),
            });
            graph.plans.push(Plan {
                ty: ty.clone(),
                expression: product(),
            });
            if held {
                graph.outputs.insert("held".into(), 1);
            }
            graph.plans.push(Plan {
                ty,
                expression: ScalarExpr::Sqrt {
                    dtype: 1,
                    value: Box::new(value),
                },
            });
            let error = render::render_cpu_elementwise(&graph, 2, "sqrt_computed").unwrap_err();
            assert!(error.contains("CPU sqrt"), "{error}");
        }
    }
}
