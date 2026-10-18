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
            include_str!("fixtures/compiler/original-gemm-norm-indexed18.json")
        }
        "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a" => {
            include_str!("fixtures/compiler/original-gemm-norm-indexed20.json")
        }
        _ => unreachable!("provider() validates identity"),
    };
    let fixture: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(fixture["library_sha256"], config.library_sha256);
    fixture
}
#[test]
fn normalized_indexed_projection_matches_independent_original_all_bytes() {
    let config = provider();
    let fixture = fixture(&config);
    assert_eq!(fixture["cache_disabled"], true);
    assert_eq!(fixture["cpu_count"], 16);
    assert_eq!(fixture["source_sha256"].as_object().unwrap().len(), 13);
    assert!(fixture["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12);
    let mut compared = 0;
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let source = decode(case["onnx_hex"].as_str().unwrap());
        for (name, key) in [("a", "input_hex"), ("features_buffer", "features_hex")] {
            assert_eq!(
                format!("{:x}", Sha256::digest(decode(case[key].as_str().unwrap()))),
                case["input_sha256"][name]
            );
        }
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
            model
                .write_input(
                    "features_buffer",
                    &decode(case["features_hex"].as_str().unwrap()),
                )
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
fn indexed_normalized_tail_requires_the_closed_unheld_feature_window() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::{chain, graph},
        compiler_onnx::lower::LoweredGraph,
    };
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-gemm-norm-indexed18.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let lowered = LoweredGraph::lower_offline(&model).unwrap();
        let selected = chain::closures(&lowered, &model).unwrap();
        assert_eq!(selected.len(), 1, "{}", case["name"]);
        let norm = selected[0].normalization.as_ref().unwrap();
        assert!(norm.indexed && !norm.packed);
        assert_eq!(selected[0].output, lowered.names["normalized"]);
        assert_eq!(
            lowered.names["current_hidden_flat"],
            lowered.names["normalized"]
        );
        let matrices = graph::closures(&lowered, &model).unwrap();
        assert_eq!(matrices.len(), 1);
        assert_eq!(matrices[0].input, lowered.names["window"]);
        assert!(selected[0].owned.contains(&matrices[0].input));
        assert!(!selected[0].owned.contains(&matrices[0].output));
        for name in [
            "terminal",
            "norm",
            "denominator",
            "current_hidden_flat",
            "current_hidden",
            "features",
            "transposed",
            "gathered",
            "window",
        ] {
            let mut held = lowered.clone();
            held.outputs
                .insert(format!("held_{name}"), held.names[name]);
            assert!(
                !chain::closures(&held, &model)
                    .unwrap()
                    .iter()
                    .any(|c| c.normalization.as_ref().is_some_and(|n| n.indexed)),
                "{} held {name}",
                case["name"]
            );
        }
        let mut changed = model.clone();
        changed.graph.initializers.get_mut("indices").unwrap().data = [-1i64, -2, -3]
            .into_iter()
            .flat_map(i64::to_le_bytes)
            .collect();
        let lowered_changed = LoweredGraph::lower_offline(&changed).unwrap();
        assert!(
            !chain::closures(&lowered_changed, &changed)
                .unwrap()
                .iter()
                .any(|c| c.normalization.as_ref().is_some_and(|n| n.indexed)),
            "{} reordered window",
            case["name"]
        );
        let mut outside = model.clone();
        let mut extra = outside.graph.nodes[0].clone();
        extra.inputs[0] = "window".into();
        extra.outputs[0] = "outside".into();
        outside.graph.nodes.push(extra);
        let mut binding = outside.graph.outputs[0].clone();
        binding.name = "outside".into();
        outside.graph.outputs.push(binding);
        let lowered_outside = LoweredGraph::lower_offline(&outside).unwrap();
        assert!(
            !chain::closures(&lowered_outside, &outside)
                .unwrap()
                .iter()
                .any(|c| c.normalization.as_ref().is_some_and(|n| n.indexed)),
            "{} outside window reader",
            case["name"]
        );
    }
}

#[test]
fn indexed_pack_replays_all_original_physical_boundaries() {
    use openpilot_model_runtime::compiler_backend_cpu::{
        llvm,
        matrix::{indexed_normalization_artifact, MatrixLayout},
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
    let mut compiled = BTreeMap::new();
    let mut exact = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let model = Model::parse(&decode(case["onnx_hex"].as_str().unwrap())).unwrap();
        let n = case["input_shape"][1].as_u64().unwrap() as usize;
        let artifact =
            indexed_normalization_artifact(n, 4, 3, "indexed_pack", "op_kernel_0").unwrap();
        let calls = case["checkpoints"].as_array().unwrap();
        let terminal = calls
            .iter()
            .rev()
            .filter(|c| !c["source_sha256"].is_null())
            .nth(1)
            .unwrap();
        assert_eq!(terminal["arguments"].as_array().unwrap().len(), 5);
        let expected = decode(terminal["snapshots"][0]["hex"].as_str().unwrap());
        let previous = |slot: usize| {
            let input = &terminal["arguments"][slot];
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
            raw[start..start + bytes].to_vec()
        };
        let args = [
            vec![0; expected.len()],
            model.graph.initializers["indices"].data.clone(),
            decode(case["features_hex"].as_str().unwrap()),
            previous(3),
            previous(4),
        ];
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
        let layout = MatrixLayout::new(&[1, 3, n], &[n, 4]).unwrap();
        for row in 0..3 {
            for channel in 0..n {
                let from = (row * layout.input_row_stride + channel) * 2;
                assert_eq!(
                    actual[from..from + 2],
                    expected[from..from + 2],
                    "{} row={row} channel={channel}",
                    case["name"]
                );
            }
        }
        exact += 1;
    }
    assert_eq!(exact, 12);
}

#[test]
fn indexed_caller_preserves_original_pack_inputs_and_immediate_output() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::MatrixLayout,
        export::{Binding, Entrypoint},
    };
    let config = provider();
    let fixture = fixture(&config);
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "gemm-norm-indexed-N8-K12-J16-payload-held0")
        .unwrap();
    let original_calls = case["checkpoints"].as_array().unwrap();
    let original_pack = original_calls
        .iter()
        .rev()
        .filter(|c| !c["source_sha256"].is_null())
        .nth(1)
        .unwrap();
    assert_eq!(original_pack["arguments"].as_array().unwrap().len(), 5);
    let previous = |slot: usize| {
        let input = &original_pack["arguments"][slot];
        let offset = input["offset"].as_u64().unwrap() as usize;
        let bytes = input["bytes"].as_u64().unwrap() as usize;
        let snapshot = original_calls
            .iter()
            .take_while(|c| c["call"] != original_pack["call"])
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
        assert_eq!(format!("{:x}", Sha256::digest(&raw)), snapshot["sha256"]);
        let start = offset - snapshot["offset"].as_u64().unwrap() as usize;
        raw[start..start + bytes].to_vec()
    };
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("model.onnx");
    let destination = temporary.path().join("bundle");
    fs::write(&path, decode(case["onnx_hex"].as_str().unwrap())).unwrap();
    let mut compilation = compile_onnx(
        &path,
        case["onnx_sha256"].as_str().unwrap(),
        &CompileOptions {
            offline_bindings: true,
            memory_planning: false,
            llvm: Some((
                config,
                PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &destination,
    )
    .unwrap();
    let packs = compilation
        .graph
        .calls
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            fs::read_to_string(destination.join(format!("kernel-{}.txt", c.kernel)))
                .unwrap()
                .contains("define void @native_indexed_pack_")
        })
        .map(|(i, c)| (i, c.views.clone()))
        .collect::<Vec<_>>();
    let [(pack_at, pack_views)] = packs.as_slice() else {
        panic!("exactly one indexed pack required")
    };
    assert_eq!(pack_views.len(), 5);
    // Add observation bindings and one-call entrypoints only after compilation.
    // Neither scheduling nor storage planning sees them. Read each boundary
    // immediately, before any later call can reuse its allocation.
    for (view, _) in compilation.graph.views.iter().enumerate() {
        compilation.graph.outputs.push(Binding {
            name: format!("__view_{view}"),
            view,
        });
    }
    compilation.graph.version = 2;
    compilation.graph.entrypoints = (0..compilation.graph.calls.len())
        .map(|i| Entrypoint {
            name: format!("__call_{i}"),
            start: i,
            end: i + 1,
        })
        .collect();
    fs::write(
        destination.join("graph.json"),
        serde_json::to_vec(&compilation.graph).unwrap(),
    )
    .unwrap();
    if let Some(directory) = std::env::var_os("MODEL_INDEXED_DIAGNOSTICS") {
        let directory = PathBuf::from(directory);
        assert!(!directory.exists(), "preserve previous diagnostics");
        fs::create_dir_all(&directory).unwrap();
        for entry in fs::read_dir(&destination).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), directory.join(entry.file_name())).unwrap();
        }
    }
    // SAFETY: unmodified trusted local kernels with the same validated views;
    // observation only partitions their original synchronous execution order.
    let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
    for (name, key) in [("a", "input_hex"), ("features_buffer", "features_hex")] {
        model
            .write_input(name, &decode(case[key].as_str().unwrap()))
            .unwrap();
    }
    let read = |model: &CpuModel, view: usize| {
        let name = format!("__view_{view}");
        let mut raw = vec![0; model.output_size(&name).unwrap()];
        model.read_output(&name, &mut raw).unwrap();
        raw
    };
    let mut failures = Vec::new();
    for i in 0..compilation.graph.calls.len() {
        if i == *pack_at {
            for (slot, name) in [(3, "terminal"), (4, "norm")] {
                let actual = read(&model, pack_views[slot]);
                let expected = previous(slot);
                if actual != expected {
                    failures.push(format!(
                        "{name} before pack: actual={actual:02x?} expected={expected:02x?}"
                    ));
                }
            }
        }
        model.run_entry(&format!("__call_{i}")).unwrap();
        if i == *pack_at {
            let actual = read(&model, pack_views[0]);
            let expected = decode(original_pack["snapshots"][0]["hex"].as_str().unwrap());
            let layout = MatrixLayout::new(&[1, 3, 8], &[8, 4]).unwrap();
            for row in 0..3 {
                let from = row * layout.input_row_stride * 2;
                if actual[from..from + 16] != expected[from..from + 16] {
                    failures.push(format!(
                        "packed row{row}: actual={:02x?} expected={:02x?}",
                        &actual[from..from + 16],
                        &expected[from..from + 16]
                    ));
                }
            }
        }
    }
    let mut actual = vec![0; model.output_size("output").unwrap()];
    model.read_output("output", &mut actual).unwrap();
    let expected = decode(case["outputs"]["output"]["hex"].as_str().unwrap());
    if actual != expected {
        failures.push(format!(
            "final matrix output: actual={actual:02x?} expected={expected:02x?}"
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
