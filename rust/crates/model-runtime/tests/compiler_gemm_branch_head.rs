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
fn fixture(config: &Config) -> Value {
    let bytes = match config.library_sha256.as_str() {
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0" => {
            include_str!("fixtures/compiler/original-gemm-branch-head18.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-gemm-branch-head20.json")
        }
        _ => unreachable!(),
    };
    let fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(
        fixture["library_sha256"], config.library_sha256,
        "independent fixture must match configured LLVM provider"
    );
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["cpu_count"], 16);
    assert_eq!(fixture["source_sha256"].as_object().unwrap().len(), 13);
    assert!(fixture["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12);
    fixture
}
#[test]
fn shared_trunk_nonaligned_concat_heads_match_independent_original_all_bytes() {
    let config = provider();
    let fixture = fixture(&config);
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
fn concat_head_ownership_keeps_one_typed_packed_trunk_and_rejects_held_heads() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::chain,
        compiler_onnx::{lower::LoweredGraph, schedule::schedule_plans},
    };
    use std::collections::{BTreeMap, BTreeSet};
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-gemm-branch-head18.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = chain::closures(&graph, &model).unwrap();
        assert_eq!(selected.len(), 2, "{}", case["name"]);
        let branch = selected.iter().find(|c| c.branch.is_some()).unwrap();
        assert_eq!(branch.output, graph.names["output"]);
        assert!(selected[0].owned.is_disjoint(&selected[1].owned));
        let mut schedule = schedule_plans(&graph, "CPU", true).unwrap();
        let kernels = chain::stage(
            &selected,
            &graph,
            &mut schedule,
            &mut BTreeMap::new(),
            &mut BTreeSet::new(),
        )
        .unwrap();
        let (&index, kernel) = kernels.iter().find(|(_, k)| k.bytes.len() == 6).unwrap();
        let call = &schedule.calls[index];
        assert_eq!(call.buffers[0], branch.output);
        assert_eq!(kernel.bytes[0], 9 * 4);
        assert_eq!(kernel.bytes[3], 3 * 2);
        assert_eq!(kernel.bytes[5], 6 * 2);
        assert_eq!(call.buffers.iter().collect::<BTreeSet<_>>().len(), 6);
        let trunk = call.buffers[1];
        assert_eq!(
            schedule
                .calls
                .iter()
                .filter(|c| c.buffers[0] == trunk)
                .count(),
            1
        );
        assert_eq!(
            schedule
                .calls
                .iter()
                .filter(|c| c.buffers[1..].contains(&trunk))
                .count(),
            1
        );
        for name in ["head3", "head6", "activated4", "add4", "r4"] {
            let mut held = graph.clone();
            held.outputs
                .insert(format!("held_{name}"), held.names[name]);
            assert!(
                !chain::closures(&held, &model)
                    .unwrap()
                    .iter()
                    .any(|c| c.branch.is_some()),
                "{} held {name}",
                case["name"]
            );
        }
        let mut outside = model.clone();
        let mut extra = outside.graph.nodes[0].clone();
        extra.inputs[0] = "activated4".into();
        extra.outputs[0] = "outside".into();
        outside.graph.nodes.push(extra);
        let mut binding = outside.graph.inputs[0].clone();
        binding.name = "outside".into();
        outside.graph.outputs.push(binding);
        let outside_graph = LoweredGraph::lower_offline(&outside).unwrap();
        assert!(
            !chain::closures(&outside_graph, &outside)
                .unwrap()
                .iter()
                .any(|c| c.branch.is_some()),
            "{} outside reader",
            case["name"]
        );
    }
}

#[test]
fn fused_concat_replays_all_original_physical_boundaries() {
    use openpilot_model_runtime::compiler_backend_cpu::{
        llvm,
        matrix::{concat_biased_artifact, MatrixLayout},
    };
    use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};
    #[repr(C, align(32))]
    struct Block([u8; 32]);
    let aligned = |raw: &[u8]| {
        raw.chunks(32)
            .map(|c| {
                let mut b = Block([0; 32]);
                b.0[..c.len()].copy_from_slice(c);
                b
            })
            .collect::<Vec<_>>()
    };
    let config = provider();
    let fixture = fixture(&config);
    assert_eq!(fixture["library_sha256"], config.library_sha256);
    let mut compiled = BTreeMap::new();
    let mut exact = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let n = case["input_shape"][1].as_u64().unwrap() as usize;
        let artifact = concat_biased_artifact(n, &[3, 6], "concat_heads", "op_kernel_0").unwrap();
        let calls = case["checkpoints"].as_array().unwrap();
        let terminal = calls
            .iter()
            .rev()
            .find(|c| !c["source_sha256"].is_null())
            .unwrap();
        assert_eq!(terminal["arguments"].as_array().unwrap().len(), 6);
        let input = &terminal["arguments"][1];
        let offset = input["offset"].as_u64().unwrap() as usize;
        let bytes = input["bytes"].as_u64().unwrap() as usize;
        let snapshot = calls
            .iter()
            .take_while(|c| c["call"] != terminal["call"])
            .flat_map(|c| c["snapshots"].as_array().unwrap())
            .filter(|s| {
                s["base"] == input["base"]
                    && s["offset"].as_u64().unwrap() as usize <= offset
                    && s["offset"].as_u64().unwrap() as usize
                        + s["bytes"].as_u64().unwrap() as usize
                        >= offset + bytes
            })
            .last()
            .unwrap();
        let raw = decode(snapshot["hex"].as_str().unwrap());
        let start = offset - snapshot["offset"].as_u64().unwrap() as usize;
        let expected = decode(terminal["snapshots"][0]["hex"].as_str().unwrap());
        assert_eq!(
            expected,
            decode(case["outputs"]["output"]["hex"].as_str().unwrap())
        );
        let mut args = vec![vec![0; expected.len()], raw[start..start + bytes].to_vec()];
        for (width, weight, bias) in [(3usize, "w10", "b10"), (6usize, "w11", "b11")] {
            let padded = width.next_multiple_of(4usize);
            let layout = MatrixLayout::new(&[1, n], &[n, padded]).unwrap();
            let mut logical = vec![0; n * padded * 2];
            let source = &model.graph.initializers[weight].data;
            for i in 0..n {
                for o in 0..width {
                    logical[(i * padded + o) * 2..(i * padded + o + 1) * 2]
                        .copy_from_slice(&source[(o * n + i) * 2..(o * n + i + 1) * 2]);
                }
            }
            let mut packed = vec![0; layout.weight_elements * 2];
            layout.pack_weights_into(&logical, &mut packed).unwrap();
            args.extend([packed, model.graph.initializers[bias].data.clone()]);
        }
        assert_eq!(
            artifact.bytes,
            args.iter().map(Vec::len).collect::<Vec<_>>()
        );
        assert_eq!(artifact.workers, 1);
        assert_eq!(artifact.scalars, 0);
        let key = format!("{:x}", Sha256::digest(&artifact.source));
        let dir = compiled.entry(key).or_insert_with(|| {
            let dir = tempfile::tempdir().unwrap();
            let obj = llvm::compile_isolated(
                Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                &config,
                &artifact.source,
                Duration::from_secs(30),
            )
            .unwrap();
            fs::write(dir.path().join("kernel.o"), obj.object).unwrap();
            let link = Command::new("cc")
                .arg("-shared")
                .arg("-Wl,-z,defs")
                .arg(dir.path().join("kernel.o"))
                .arg("-o")
                .arg(dir.path().join("kernel.so"))
                .output()
                .unwrap();
            assert!(
                link.status.success(),
                "{}",
                String::from_utf8_lossy(&link.stderr)
            );
            dir
        });
        let mut buffers = args.iter().map(|b| aligned(b)).collect::<Vec<_>>();
        let pointers = buffers
            .iter_mut()
            .map(|b| b.as_mut_ptr().cast())
            .collect::<Vec<_>>();
        // SAFETY: independently checked source ABI, disjoint aligned capacities,
        // trusted local kernel and synchronous execution while all buffers live.
        unsafe {
            let lib = libloading::Library::new(dir.path().join("kernel.so")).unwrap();
            let run: libloading::Symbol<
                unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32),
            > = lib.get(b"op_kernel_0").unwrap();
            run(pointers.as_ptr(), std::ptr::null());
        }
        let actual = buffers[0]
            .iter()
            .flat_map(|b| b.0)
            .take(expected.len())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{}", case["name"]);
        exact += 1;
    }
    assert_eq!(exact, 12);
}
