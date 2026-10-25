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
fn source(c: &Value) -> Vec<u8> {
    let k = c["channels"].as_u64().unwrap() as usize;
    let tensor = |name: &str, shape: &[usize], data: &[u8]| {
        message(
            5,
            &[
                shape.iter().flat_map(|v| field(1, *v)).collect::<Vec<_>>(),
                field(2, 1),
                message(8, name.as_bytes()),
                message(9, data),
            ]
            .concat(),
        )
    };
    let vi = |tag: usize, name: &str| {
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
                                &[message(1, &field(1, 1)), message(1, &field(1, k))].concat(),
                            ),
                        ]
                        .concat(),
                    ),
                ),
            ]
            .concat(),
        )
    };
    let node = |op: &str, a: &str, b: &str, out: &str| {
        message(
            1,
            &[
                message(1, a.as_bytes()),
                message(1, b.as_bytes()),
                message(2, out.as_bytes()),
                message(4, op.as_bytes()),
            ]
            .concat(),
        )
    };
    let weights = decode(c["weight_pattern_hex"].as_str().unwrap())
        .into_iter()
        .cycle()
        .take(k * k * 4)
        .collect::<Vec<_>>();
    let mut graph = [
        node("Mul", "a", "scale", "x"),
        node("MatMul", "x", "w", "matrix"),
        node("Add", "matrix", "x", "output"),
        tensor("scale", &[], &1.0001f32.to_le_bytes()),
        tensor("w", &[k, k], &weights),
        vi(11, "a"),
        vi(12, "output"),
    ]
    .concat();
    if c["held"].as_bool().unwrap() {
        graph.extend(vi(12, "x"));
    }
    let data = [message(8, &field(2, 20)), message(7, &graph)].concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        c["onnx_sha256"].as_str().unwrap()
    );
    data
}
fn original_case_for_provider<'a>(fixture: &'a Value, case: &Value, library: &str) -> &'a Value {
    let expected = &fixture["provider_cases"][library][case["name"].as_str().unwrap()];
    assert!(
        expected.is_object(),
        "missing independent original provider receipt {library} {}",
        case["name"]
    );
    if library == fixture["library_sha256"].as_str().unwrap() {
        assert_eq!(expected["outputs"], case["outputs"]);
        assert_eq!(expected["matrix_output_hex"], case["matrix_output_hex"]);
    }
    expected
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
fn original_direct_computed_matrix_residual_storage_and_held_outputs() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-matrix-alias.json")).unwrap();
    let config = provider();
    // Compile each distinct source once per memory policy; all original inputs,
    // weights, held outputs, NaN payloads and signed zeros remain asserted.
    let mut sources = std::collections::BTreeMap::<String, Vec<&Value>>::new();
    for c in f["cases"].as_array().unwrap() {
        sources
            .entry(c["onnx_sha256"].as_str().unwrap().into())
            .or_default()
            .push(c);
    }
    for (hash, cases) in sources {
        let data = source(cases[0]);
        for memory_planning in [false, true] {
            eprintln!("matrix alias {} memory={memory_planning}", cases[0]["name"]);
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
            // SAFETY: locally compiled immutable artifact with exact declared input bytes.
            let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
            for c in &cases {
                assert_eq!(source(c), data);
                model
                    .write_input("a", &decode(c["input_hex"].as_str().unwrap()))
                    .unwrap();
                model.run();
                for (name, want) in original_case_for_provider(&f, c, &config.library_sha256)
                    ["outputs"]
                    .as_object()
                    .unwrap()
                {
                    let mut raw = vec![0; model.output_size(name).unwrap()];
                    model.read_output(name, &mut raw).unwrap();
                    let want = decode(want["hex"].as_str().unwrap());
                    assert!(raw == want,
                        "{} output={name} memory={memory_planning} firstbyte={:?} native={:x} original={:x}",
                        c["name"], raw.iter().zip(&want).position(|(a,b)|a!=b), Sha256::digest(&raw), Sha256::digest(&want));
                }
            }
        }
    }
}

#[test]
fn original_matrix_residual_physical_kernel_exact_bytes() {
    use openpilot_model_runtime::compiler_backend_cpu::{
        llvm::{compile_isolated, Config},
        matrix::MatrixLayout,
    };
    use std::{path::PathBuf, process::Command, time::Duration};
    #[repr(C, align(32))]
    #[derive(Clone)]
    struct Block([u8; 32]);
    let aligned = |raw: &[u8]| {
        let mut out = vec![Block([0; 32]); raw.len().div_ceil(32)];
        for (i, b) in raw.iter().enumerate() {
            out[i / 32].0[i % 32] = *b;
        }
        out
    };
    let f: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-matrix-alias.json")).unwrap();
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicitLLVM_PATHrequired"));
    let config = Config {
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
    };
    for k in [4, 8, 32, 512] {
        let layout = MatrixLayout::new(&[1, k], &[k, k]).unwrap();
        let kernel = layout
            .residual_artifact("matrix_residual", "op_kernel_0", 16)
            .unwrap();
        let compiled = compile_isolated(
            std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            &config,
            &kernel.source,
            Duration::from_secs(30),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("kernel.o"), compiled.object).unwrap();
        let status = Command::new("cc")
            .arg("-shared")
            .arg("-Wl,-z,defs")
            .arg(temp.path().join("kernel.o"))
            .arg("-o")
            .arg(temp.path().join("kernel.so"))
            .status()
            .unwrap();
        assert!(status.success());
        for c in f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["channels"] == k)
        {
            let input_raw = decode(c["packed_input_valid_hex"].as_str().unwrap());
            let mut input = vec![0xa3; kernel.bytes[1]];
            input[..input_raw.len()].copy_from_slice(&input_raw);
            let half = decode(c["weight_pattern_hex"].as_str().unwrap())
                .chunks_exact(4)
                .map(|p| {
                    let bits = u32::from_le_bytes(p.try_into().unwrap());
                    let magnitude = bits & 0x7fff_ffff;
                    assert!(magnitude == 0 || (0x3880_0000..0x4780_0000).contains(&magnitude));
                    ((bits >> 16) & 0x8000
                        | if magnitude == 0 {
                            0
                        } else {
                            ((magnitude >> 13) - 0x1c000) & 0x7fff
                        }) as u16
                })
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
                .into_iter()
                .cycle()
                .take(k * k * 2)
                .collect::<Vec<_>>();
            assert_eq!(
                format!("{:x}", Sha256::digest(&half)),
                c["materialized_weights_sha256"].as_str().unwrap()
            );
            let mut weight = vec![0xa3; kernel.bytes[2]];
            layout.pack_weights_into(&half, &mut weight).unwrap();
            let input = aligned(&input);
            let weight = aligned(&weight);
            let mut output = aligned(&vec![0x55; kernel.bytes[0]]);
            let ptrs = [
                output.as_mut_ptr().cast::<std::ffi::c_void>(),
                input.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
                weight.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
            ];
            // SAFETY: exact locally generated synchronous3buffer/1scalarABI, disjointalignedstorage.
            unsafe {
                let lib = libloading::Library::new(temp.path().join("kernel.so")).unwrap();
                let run = lib
                    .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(
                        b"op_kernel_0\0",
                    )
                    .unwrap();
                for worker in 0..kernel.workers {
                    run(ptrs.as_ptr(), &(worker as i32));
                }
            }
            let out = output
                .into_iter()
                .flat_map(|b| b.0)
                .take(kernel.bytes[0])
                .collect::<Vec<_>>();
            let expected = decode(
                original_case_for_provider(&f, c, &config.library_sha256)["matrix_output_hex"]
                    .as_str()
                    .unwrap(),
            );
            assert!(
                out == expected,
                "{} firstbyte={:?} native={:x} original={:x}",
                c["name"],
                out.iter().zip(&expected).position(|(a, b)| a != b),
                Sha256::digest(&out),
                Sha256::digest(&expected)
            );
        }
    }
}

#[test]
fn matrix_residual_without_llvm_keeps_shared_storage_fence() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-matrix-alias.json")).unwrap();
    let c = &f["cases"][0];
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("model.onnx");
    fs::write(&path, source(c)).unwrap();
    let destination = temp.path().join("bundle");
    let error = compile_onnx(
        &path,
        c["onnx_sha256"].as_str().unwrap(),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &destination,
    )
    .unwrap_err();
    assert!(error.contains("shared view storage"), "{error}");
    assert!(!destination.exists());
}

#[test]
fn matrix_residual_pipeline_owns_pack_and_dense_output_abi() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::graph::{closures, stage},
        compiler_onnx::{lower::LoweredGraph, schedule::schedule_plans, Model},
    };
    use std::collections::{BTreeMap, BTreeSet};
    let f: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-matrix-alias.json")).unwrap();
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["mode"] == "finite")
    {
        let model = Model::parse(&source(c)).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = closures(&graph, &model).unwrap();
        assert_eq!(selected.len(), 1);
        assert!(selected[0].residual);
        let mut scheduled = schedule_plans(&graph, "CPU", true).unwrap();
        let mut initialized = BTreeMap::new();
        let mut scratch = BTreeSet::new();
        let calls = stage(
            &selected,
            &graph,
            &mut scheduled,
            &mut initialized,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(scratch.len(), 1);
        assert_eq!(initialized.len(), 1);
        let pack_at = *calls.keys().next().unwrap();
        let reduce_at = pack_at + 1;
        assert_eq!(scheduled.calls[pack_at].buffers[1], selected[0].input);
        assert_eq!(
            scheduled.calls[reduce_at].buffers[1],
            scheduled.calls[pack_at].buffers[0]
        );
        assert_eq!(scheduled.calls[reduce_at].buffers[0], selected[0].output);
        assert!(!scheduled
            .calls
            .iter()
            .any(|call| call.buffers[0] == selected[0].reduction));
        assert_eq!(calls[&pack_at].workers, 1);
        assert_eq!(calls[&pack_at].scalars, 0);
        assert_eq!(calls[&reduce_at].scalars, 1);
        for (index, call) in calls {
            assert_eq!(
                call.bytes,
                scheduled.calls[index]
                    .buffers
                    .iter()
                    .map(|id| scheduled.buffers[*id].bytes)
                    .collect::<Vec<_>>()
            );
        }
    }
}
