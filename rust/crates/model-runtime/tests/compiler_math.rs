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
use std::{fs, process::Command};
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/compiler/original-math.json")).unwrap()
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
fn native_math_primitives_match_original_dynamic_float32_bits() {
    let f = fixture();
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["onnx_hex"].is_null())
    {
        let inputs = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| decode(v["data_hex"].as_str().unwrap()))
            .collect::<Vec<_>>();
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
        let expression = match case["name"].as_str().unwrap() {
            "Exp2" => ScalarExpr::Exp2 {
                dtype: 1,
                value: load(0),
            },
            "Sign" => ScalarExpr::Sign {
                dtype: 1,
                value: load(0),
            },
            "MulAcc" => ScalarExpr::MulAcc {
                dtype: 1,
                a: load(0),
                b: load(1),
                c: load(2),
            },
            _ => unreachable!(),
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
fn native_onnx_activations_match_original_dynamic_float32_bits() {
    let f = fixture();
    for (index, case) in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["onnx_hex"].is_null() && c["cpu_admission"] == "supported")
        .enumerate()
    {
        let directory = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = directory.path().join(format!("model{index}.onnx"));
        fs::write(&onnx, &bytes).unwrap();
        let bundle = directory.path().join("bundle");
        compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions::default(),
            &bundle,
        )
        .unwrap();
        let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
        for input in case["inputs"].as_array().unwrap() {
            model
                .write_input(
                    input["name"].as_str().unwrap(),
                    &decode(input["data_hex"].as_str().unwrap()),
                )
                .unwrap();
        }
        model.run();
        let mut output = vec![0; model.output_size("output").unwrap()];
        model.read_output("output", &mut output).unwrap();
        assert_same_bits(
            &output,
            &decode(case["expected_hex"].as_str().unwrap()),
            case["name"].as_str().unwrap(),
        );
    }
}
#[test]
fn actual_first_driver_gelu_matches_original_full_output_hash() {
    let f = fixture();
    let case = &f["actual_driver_gelu"];
    let bytes = decode(case["onnx_hex"].as_str().unwrap());
    let parsed = openpilot_model_runtime::compiler_onnx::Model::parse(&bytes).unwrap();
    let name = &parsed.graph.outputs[0].name;
    let directory = tempfile::tempdir().unwrap();
    let onnx = directory.path().join("driver.onnx");
    fs::write(&onnx, &bytes).unwrap();
    let bundle = directory.path().join("bundle");
    compile_onnx(
        &onnx,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &bundle,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
    model
        .write_input("input_img", &(0u8..=255).collect::<Vec<_>>().repeat(5400))
        .unwrap();
    model.run();
    let mut output = vec![0; model.output_size(name).unwrap()];
    model.read_output(name, &mut output).unwrap();
    assert_eq!(output.len() as u64, case["output_bytes"].as_u64().unwrap());
    assert_eq!(
        format!("{:x}", Sha256::digest(output)),
        case["output_sha256"].as_str().unwrap()
    );
}

#[test]
fn unproved_erf_gelu_layouts_fail_before_compilation_or_publication() {
    let f = fixture();
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["cpu_admission"] == "pending_layout_sensitive_nan")
    {
        let directory = tempfile::tempdir().unwrap();
        let bytes = decode(case["onnx_hex"].as_str().unwrap());
        let onnx = directory.path().join("none.onnx");
        fs::write(&onnx, &bytes).unwrap();
        let destination = directory.path().join("bundle");
        let error = compile_onnx(
            &onnx,
            &format!("{:x}", Sha256::digest(&bytes)),
            &CompileOptions {
                program: directory.path().join("must-not-execute"),
                ..CompileOptions::default()
            },
            &destination,
        )
        .unwrap_err();
        assert!(
            error.contains("layout-sensitive NaN"),
            "{}: {error}",
            case["name"]
        );
        assert!(!destination.exists());
    }
}

#[test]
fn math_rendering_keeps_unproved_dtypes_and_gpu_targets_fail_closed() {
    let mut graph = LoweredGraph::default();
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![1],
        },
        expression: ScalarExpr::Input("a".into()),
    });
    let load = || {
        Box::new(ScalarExpr::Load {
            plan: 0,
            index: IndexExpr::Index,
        })
    };
    graph.plans.push(Plan {
        ty: TensorType {
            dtype: 1,
            shape: vec![1],
        },
        expression: ScalarExpr::Exp2 {
            dtype: 1,
            value: load(),
        },
    });
    assert!(render::render_elementwise(&graph, 1, "gpu_exp2").is_err());
    for expression in [
        ScalarExpr::Exp2 {
            dtype: 10,
            value: load(),
        },
        ScalarExpr::Sign {
            dtype: 10,
            value: load(),
        },
        ScalarExpr::MulAcc {
            dtype: 10,
            a: load(),
            b: load(),
            c: load(),
        },
    ] {
        graph.plans[1].ty.dtype = 10;
        graph.plans[1].expression = expression;
        assert!(render::render_cpu_elementwise(&graph, 1, "unproved_half_math").is_err());
    }
}
