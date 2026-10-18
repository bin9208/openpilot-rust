#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, llvm::Config, matrix::MatrixLayout, CompileOptions},
    compiler_onnx::Model,
    CpuModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/compiler/original-batched-matmul.json"
    ))
    .unwrap()
}
fn provider() -> Config {
    let library =
        PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH required"));
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
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn scalar_field(n: usize, v: usize) -> Vec<u8> {
    [varint(n * 8), varint(v)].concat()
}
fn value_info(field: usize, name: &str, shape: &[usize], dtype: usize) -> Vec<u8> {
    let shape = shape
        .iter()
        .flat_map(|v| message(1, &scalar_field(1, *v)))
        .collect::<Vec<_>>();
    message(
        field,
        &[
            message(1, name.as_bytes()),
            message(
                2,
                &message(1, &[scalar_field(1, dtype), message(2, &shape)].concat()),
            ),
        ]
        .concat(),
    )
}
fn synthetic_source(case: &Value) -> Vec<u8> {
    let dims = |key: &str| {
        case[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>()
    };
    let x = dims("input_shape");
    let w = dims("weight_shape");
    let y = dims("output_shape");
    let raw = decode(case["weight_pattern_hex"].as_str().unwrap())
        .into_iter()
        .cycle()
        .take(w.iter().product::<usize>() * 2)
        .collect::<Vec<_>>();
    let tensor = [
        w.iter()
            .flat_map(|v| scalar_field(1, *v))
            .collect::<Vec<_>>(),
        scalar_field(2, 10),
        message(8, b"w"),
        message(9, &raw),
    ]
    .concat();
    let node = message(
        1,
        &[
            message(1, b"a"),
            message(1, b"w"),
            message(2, b"output"),
            message(4, b"MatMul"),
        ]
        .concat(),
    );
    let source = [
        message(8, &scalar_field(2, 20)),
        message(
            7,
            &[
                node,
                message(5, &tensor),
                value_info(11, "a", &x, 1),
                value_info(12, "output", &y, 1),
            ]
            .concat(),
        ),
    ]
    .concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        case["onnx_sha256"].as_str().unwrap()
    );
    source
}

#[test]
fn original_batched_matmul_full_compiler_artifacts() {
    let config = provider();
    for case in fixture()["cases"].as_array().unwrap() {
        let source = synthetic_source(case);
        for memory_planning in [false, true] {
            eprintln!("matrix {} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("model.onnx");
            fs::write(&path, &source).unwrap();
            let destination = temp.path().join("bundle");
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
            )
            .unwrap();
            assert_eq!(compilation.graph.backend, "cpu-llvm");
            // SAFETY: locally emitted immutable synchronous artifact; original
            // physical input length matches its pinned graph declaration.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
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
// Lossless fixture construction from the mandatory pinned model, avoiding a
// duplicate multi-megabyte copy of its weights. Both source and constructed
// prefix identities are independently captured original SHA256 values.
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

#[test]
fn actual_initializer_materialization_receipt() {
    let fixture = fixture();
    let receipt = &fixture["actual_prefix_receipts"][0];
    let source = actual_prefix_source(receipt);
    let model = Model::parse(&source).unwrap();
    let weight = &model.graph.initializers["policy/val_12"];
    assert_eq!(weight.shape, [512, 512]);
    assert_eq!(weight.dtype, 10);
    assert_eq!(
        format!("{:x}", Sha256::digest(&weight.data)),
        receipt["initializer"]["sha256"].as_str().unwrap()
    );
    let layout = MatrixLayout::new(&[1, 9, 512], &[512, 512]).unwrap();
    let mut packed = vec![0xa3; layout.weight_elements * 2];
    layout.pack_weights_into(&weight.data, &mut packed).unwrap();
    let mut recovered = vec![0; weight.data.len()];
    for k in 0..512 {
        for n in 0..512 {
            let from = ((n / 4) * layout.weight_output_stride + k * 4 + n % 4) * 2;
            let to = (k * 512 + n) * 2;
            recovered[to..to + 2].copy_from_slice(&packed[from..from + 2]);
        }
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(&recovered)),
        receipt["original_materialized_weights_sha256"]
            .as_str()
            .unwrap()
    );
}

#[test]
fn matrix_pipeline_dependencies_and_partition_contract() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::graph::{closures, stage},
        compiler_onnx::{lower::LoweredGraph, schedule::schedule_plans},
    };
    use std::collections::{BTreeMap, BTreeSet};
    for case in fixture()["cases"].as_array().unwrap() {
        let model = Model::parse(&synthetic_source(case)).unwrap();
        let graph = LoweredGraph::lower_offline(&model).unwrap();
        let selected = closures(&graph, &model).unwrap();
        assert_eq!(selected.len(), 1);
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
        assert_eq!(scheduled.calls.len(), 3);
        assert_eq!(calls.len(), 3);
        assert_eq!(scratch.len(), 2);
        assert_eq!(initialized.len(), 1);
        assert_eq!(scheduled.calls[0].buffers[1], selected[0].input);
        assert_eq!(scheduled.calls[1].buffers[1], scheduled.calls[0].buffers[0]);
        assert_eq!(scheduled.calls[2].buffers[1], scheduled.calls[1].buffers[0]);
        assert_eq!(scheduled.calls[2].buffers[0], selected[0].output);
        for (index, call) in &calls {
            assert_eq!(
                scheduled.calls[*index]
                    .buffers
                    .iter()
                    .map(|id| scheduled.buffers[*id].bytes)
                    .collect::<Vec<_>>(),
                call.bytes
            );
            assert_eq!(call.scalars, usize::from(*index == 1));
            if *index != 1 {
                assert_eq!(call.workers, 1);
            }
        }
        assert_eq!(
            calls[&1].workers,
            selected[0].layout.schedule(16).unwrap().workers
        );
    }
}

#[repr(C, align(32))]
#[derive(Clone)]
struct Block([u8; 32]);
fn aligned(raw: &[u8]) -> Vec<Block> {
    raw.chunks(32)
        .map(|chunk| {
            let mut b = Block([0; 32]);
            b.0[..chunk.len()].copy_from_slice(chunk);
            b
        })
        .collect()
}
#[test]
fn actual_driving_prefix124_matches_cold_original() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-driving-prefix124.json"
    ))
    .unwrap();
    let config = provider();
    let receipt = fixture["provider_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["library_sha256"] == config.library_sha256)
        .expect("independently executed original provider required");
    assert_eq!(receipt["cache_disabled"], true);
    assert_eq!(receipt["compile_to_obj_calls"], 66);
    assert_eq!(receipt["program_calls"], 131);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, expected) in receipt["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(name)).unwrap())),
            expected.as_str().unwrap(),
            "original source drift: {name}"
        );
    }
    let source = actual_prefix_source(receipt);
    let inputs = [
        ("img", (0u8..=255).collect::<Vec<_>>().repeat(1536)),
        (
            "big_img",
            (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
        ),
        ("desire_pulse", vec![0; 25 * 8 * 4]),
        ("traffic_convention", vec![0; 8]),
        ("features_buffer", vec![0; 24 * 512 * 4]),
        ("action_t", vec![0; 8]),
    ];
    for (name, raw) in &inputs {
        assert_eq!(
            format!("{:x}", Sha256::digest(raw)),
            receipt["input_sha256"][name].as_str().unwrap()
        );
    }
    for memory_planning in [false, true] {
        eprintln!("actual prefix124 memory={memory_planning}");
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prefix.onnx");
        fs::write(&path, &source).unwrap();
        let destination = temp.path().join("bundle");
        let mut compiled = compile_onnx(
            &path,
            receipt["prefix_sha256"].as_str().unwrap(),
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
        )
        .unwrap();
        assert_eq!(compiled.graph.backend, "cpu-llvm");
        // Each policy owns a separate immutable observation directory. Earlier
        // numerical RED stopped before policy=true; sharing its directory would
        // otherwise turn the first numerical GREEN into an observer failure.
        let policy_directory = |root| PathBuf::from(root).join(format!("memory-{memory_planning}"));
        let diagnostic = std::env::var_os("MODEL_PREFIX124_DIAGNOSTICS").map(policy_directory);
        let checkpoints = std::env::var_os("MODEL_PREFIX124_CHECKPOINTS").map(policy_directory);
        if let Some(directory) = &checkpoints {
            assert!(!directory.exists(), "preserve earlier checkpoint evidence");
            fs::create_dir_all(directory).unwrap();
            assert_eq!(compiled.graph.version, 1);
            assert!(compiled.graph.entrypoints.is_empty());
            // Partition the already compiled calls without changing their order,
            // ABI, workers, storage or source. run_entry uses the same executor
            // as run, and each output is read before any later arena reuse.
            fs::write(
                directory.join("original-graph.json"),
                serde_json::to_vec_pretty(&compiled.graph).unwrap(),
            )
            .unwrap();
            compiled.graph.version = 2;
            for (index, call) in compiled.graph.calls.iter().enumerate() {
                let name = format!("__prefix124_call_{index}");
                compiled.graph.entrypoints.push(
                    serde_json::from_value(serde_json::json!({
                        "name": name,
                        "start": index,
                        "end": index + 1,
                    }))
                    .unwrap(),
                );
                compiled
                    .graph
                    .outputs
                    .push(openpilot_model_runtime::export::Binding {
                        name,
                        view: call.views[0],
                    });
                let source = format!("kernel-{}.txt", call.kernel);
                fs::copy(destination.join(&source), directory.join(&source)).unwrap();
            }
            fs::write(
                directory.join("observed-graph.json"),
                serde_json::to_vec_pretty(&compiled.graph).unwrap(),
            )
            .unwrap();
            fs::write(
                destination.join("graph.json"),
                serde_json::to_vec(&compiled.graph).unwrap(),
            )
            .unwrap();
        }
        let mut ancestry = Vec::new();
        if diagnostic.is_some() {
            let output = compiled
                .graph
                .outputs
                .iter()
                .find(|b| b.name == "vision/add_12")
                .unwrap()
                .view;
            let call = compiled
                .graph
                .calls
                .iter()
                .find(|c| c.views[0] == output)
                .unwrap();
            assert_eq!(call.workers, 4);
            assert_eq!(call.core_id, Some(0));
            assert_eq!(call.scalars.len(), 1);
            assert_eq!(
                call.views
                    .iter()
                    .map(|id| compiled.graph.views[*id].bytes)
                    .collect::<Vec<_>>(),
                [2048, 1088, 2112, 1114112, 1024]
            );
            let views = call.views.clone();
            let root_pack = compiled
                .graph
                .calls
                .iter()
                .find(|c| c.views[0] == views[1])
                .unwrap();
            assert_eq!(root_pack.views.len(), 2);
            let logical_root = root_pack.views[1];
            let mut pending = vec![(logical_root, 0usize)];
            while let Some((view, depth)) = pending.pop() {
                let name = format!("ancestry_{}", ancestry.len());
                ancestry.push((name, view));
                if depth < 11 {
                    if let Some(producer) = compiled.graph.calls.iter().find(|c| c.views[0] == view)
                    {
                        eprintln!(
                            "prefix124 lineage depth={depth} view={view} kernel={} views={:?} bytes={:?}",
                            producer.kernel,
                            producer.views,
                            producer.views.iter().map(|id| compiled.graph.views[*id].bytes).collect::<Vec<_>>()
                        );
                        pending.extend(producer.views[1..].iter().map(|v| (*v, depth + 1)));
                    }
                }
            }
            for (name, view) in &ancestry {
                compiled
                    .graph
                    .outputs
                    .push(openpilot_model_runtime::export::Binding {
                        name: format!("__prefix124_{name}"),
                        view: *view,
                    });
            }
            for (name, view) in ["output", "root", "hidden", "weight", "bias"]
                .into_iter()
                .zip(views)
            {
                compiled
                    .graph
                    .outputs
                    .push(openpilot_model_runtime::export::Binding {
                        name: format!("__prefix124_{name}"),
                        view,
                    });
            }
            // Observation only: the planner and compiler have already finished.
            // Calls, allocation lifetimes, weights and kernel receipts are intact.
            fs::write(
                destination.join("graph.json"),
                serde_json::to_vec(&compiled.graph).unwrap(),
            )
            .unwrap();
        }
        // SAFETY: current locally compiled artifact, pinned model/provider, and
        // exact original physical inputs; execution is synchronous.
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        for (name, raw) in &inputs {
            // This shorter prefix prunes unused policy inputs. Their original
            // hashes remain checked above; write every retained binding only.
            if compiled
                .graph
                .inputs
                .iter()
                .any(|binding| binding.name == *name)
            {
                model.write_input(name, raw).unwrap();
            }
        }
        if let Some(directory) = &checkpoints {
            let mut records = Vec::new();
            let mut total_bytes = 0usize;
            for (index, call) in compiled.graph.calls.iter().enumerate() {
                let name = format!("__prefix124_call_{index}");
                model.run_entry(&name).unwrap();
                let mut data = vec![0; model.output_size(&name).unwrap()];
                model.read_output(&name, &mut data).unwrap();
                total_bytes += data.len();
                assert!(total_bytes <= 256 * 1024 * 1024);
                let file = format!("call-{index:03}-slot-0.bin");
                fs::write(directory.join(&file), &data).unwrap();
                records.push(serde_json::json!({
                    "call": index,
                    "kernel": call.kernel,
                    "views": call.views,
                    "bytes": data.len(),
                    "sha256": format!("{:x}", Sha256::digest(&data)),
                    "private_snapshot": file,
                }));
            }
            fs::write(
                directory.join("checkpoints.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        } else {
            model.run();
        }
        if let Some(directory) = diagnostic {
            fs::create_dir_all(&directory).unwrap();
            let mut copied = std::collections::BTreeSet::new();
            for (_, view) in &ancestry {
                if let Some(call) = compiled.graph.calls.iter().find(|c| c.views[0] == *view) {
                    if copied.insert(call.kernel) {
                        let name = format!("kernel-{}.txt", call.kernel);
                        fs::copy(destination.join(&name), directory.join(&name)).unwrap();
                    }
                }
            }
            fs::write(
                directory.join("observed-graph.json"),
                serde_json::to_vec_pretty(&compiled.graph).unwrap(),
            )
            .unwrap();
            for name in ["output", "root", "hidden", "weight", "bias"]
                .into_iter()
                .map(str::to_owned)
                .chain(ancestry.iter().map(|(name, _)| name.clone()))
            {
                let binding = format!("__prefix124_{name}");
                let mut data = vec![0; model.output_size(&binding).unwrap()];
                model.read_output(&binding, &mut data).unwrap();
                fs::write(
                    directory.join(format!("memory{memory_planning}-{name}.bin")),
                    &data,
                )
                .unwrap();
                eprintln!(
                    "prefix124 observed {name} bytes={} sha256={:x}",
                    data.len(),
                    Sha256::digest(&data)
                );
            }
        }
        let name = receipt["output_name"].as_str().unwrap();
        let mut output = vec![0; model.output_size(name).unwrap()];
        assert_eq!(
            output.len(),
            receipt["output_bytes"].as_u64().unwrap() as usize
        );
        model.read_output(name, &mut output).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&output)),
            receipt["output_sha256"].as_str().unwrap(),
            "actual prefix124 memory={memory_planning}"
        );
    }
}

#[test]
fn actual_node274_materialized_kernel_matches_original() {
    use openpilot_model_runtime::compiler_backend_cpu::llvm::compile_isolated;
    use std::{process::Command, time::Duration};
    let fixture = fixture();
    let config = provider();
    let receipt = fixture["actual_prefix_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["library_sha256"] == config.library_sha256)
        .unwrap();
    let source = actual_prefix_source(receipt);
    let model = Model::parse(&source).unwrap();
    let layout = MatrixLayout::new(&[1, 9, 512], &[512, 512]).unwrap();
    let kernel = layout.artifact("actual_matrix", "op_kernel_0", 16).unwrap();
    let valid = decode(fixture["actual_valid_packed_input_hex"].as_str().unwrap());
    assert_eq!(
        format!("{:x}", Sha256::digest(&valid)),
        receipt["valid_packed_input_sha256"].as_str().unwrap()
    );
    let mut input = vec![0xa3; kernel.bytes[1]];
    for row in 0..9 {
        input[row * 1088..row * 1088 + 1024].copy_from_slice(&valid[row * 1024..(row + 1) * 1024]);
    }
    let mut weights = vec![0xa3; kernel.bytes[2]];
    layout
        .pack_weights_into(
            &model.graph.initializers["policy/val_12"].data,
            &mut weights,
        )
        .unwrap();
    let compiled = compile_isolated(
        std::path::Path::new(env!("CARGO_BIN_EXE_offline-llvm-compile")),
        &config,
        &kernel.source,
        Duration::from_secs(30),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("kernel.o"), compiled.object).unwrap();
    let linked = Command::new("cc")
        .arg("-shared")
        .arg("-Wl,-z,defs")
        .arg(dir.path().join("kernel.o"))
        .arg("-o")
        .arg(dir.path().join("kernel.so"))
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let input = aligned(&input);
    let weights = aligned(&weights);
    let mut output = aligned(&vec![0x55; kernel.bytes[0]]);
    let pointers = [
        output.as_mut_ptr().cast::<std::ffi::c_void>(),
        input.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
        weights.as_ptr().cast::<std::ffi::c_void>().cast_mut(),
    ];
    // SAFETY: exact locally emitted synchronous three-buffer/one-scalar ABI;
    // all buffers are disjoint,32-byte aligned, and have declared byte lengths.
    unsafe {
        let module = libloading::Library::new(dir.path().join("kernel.so")).unwrap();
        let run = module
            .get::<unsafe extern "C" fn(*const *mut std::ffi::c_void, *const i32)>(b"op_kernel_0\0")
            .unwrap();
        for worker in 0..kernel.workers {
            run(pointers.as_ptr(), &(worker as i32));
        }
    }
    let actual = output
        .iter()
        .flat_map(|b| b.0)
        .take(kernel.bytes[0])
        .collect::<Vec<_>>();
    assert_eq!(
        format!("{:x}", Sha256::digest(&actual)),
        receipt["output_sha256"].as_str().unwrap()
    );
}
#[test]
fn actual_driving_node274_true_prefix_matches_original() {
    let fixture = fixture();
    let config = provider();
    let receipt = fixture["actual_prefix_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["library_sha256"] == config.library_sha256)
        .unwrap();
    let source = actual_prefix_source(receipt);
    let inputs = [
        ("img", (0u8..=255).collect::<Vec<_>>().repeat(1536)),
        (
            "big_img",
            (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
        ),
        ("desire_pulse", vec![0; 25 * 8 * 4]),
        ("traffic_convention", vec![0; 8]),
        ("features_buffer", vec![0; 24 * 512 * 4]),
        ("action_t", vec![0; 8]),
    ];
    for (name, raw) in &inputs {
        assert_eq!(
            format!("{:x}", Sha256::digest(raw)),
            receipt["inputs"][name]["sha256"].as_str().unwrap()
        );
    }
    for memory_planning in [false, true] {
        eprintln!("actual prefix274 memory={memory_planning}");
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prefix.onnx");
        fs::write(&path, &source).unwrap();
        let destination = temp.path().join("bundle");
        let mut compilation = compile_onnx(
            &path,
            receipt["prefix_sha256"].as_str().unwrap(),
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
        )
        .unwrap();
        assert!(
            compilation
                .graph
                .calls
                .iter()
                .any(|c| c.workers == 16 && c.core_id == Some(0)),
            "matrix partition ABI missing"
        );
        let packed_input = compilation
            .graph
            .calls
            .iter()
            .find(|c| c.workers == 16 && c.core_id == Some(0))
            .unwrap()
            .views[1];
        // Expose the already compiled scratch view for observation only. This
        // happens after scheduling/compilation: it cannot force an intermediate
        // realization or change the final-output-only original context.
        compilation
            .graph
            .outputs
            .push(openpilot_model_runtime::export::Binding {
                name: "__matrix_input".into(),
                view: packed_input,
            });
        fs::write(
            destination.join("graph.json"),
            serde_json::to_vec(&compilation.graph).unwrap(),
        )
        .unwrap();
        // SAFETY: locally compiled immutable artifact, executed synchronously with
        // exact physical input byte lengths from the original capture receipt.
        let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
        for (name, raw) in &inputs {
            model.write_input(name, raw).unwrap();
        }
        model.run();
        let mut packed = vec![0; model.output_size("__matrix_input").unwrap()];
        model.read_output("__matrix_input", &mut packed).unwrap();
        assert_eq!(packed.len(), 9792);
        let valid = (0..9)
            .flat_map(|row| packed[row * 1088..row * 1088 + 1024].iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(
            format!("{:x}", Sha256::digest(&valid)),
            receipt["valid_packed_input_sha256"].as_str().unwrap(),
            "actualprefix274 input staging memory={memory_planning}"
        );
        let name = receipt["output_name"].as_str().unwrap();
        let mut output = vec![0; model.output_size(name).unwrap()];
        model.read_output(name, &mut output).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&output)),
            receipt["output_sha256"].as_str().unwrap(),
            "actualprefix274 memory={memory_planning}"
        );
    }
}

fn control_source(case: &Value) -> Vec<u8> {
    let dims = |key: &str| {
        case[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>()
    };
    let x = dims("input_shape");
    let matrix = dims("matrix_shape");
    let w = dims("weight_shape");
    let name = case["name"].as_str().unwrap();
    let held = name.contains("held");
    let transposed = name.starts_with("transpose");
    let flattened = name.starts_with("flatten");
    let tensor = |name: &str, shape: &[usize], raw: &[u8], dtype| {
        message(
            5,
            &[
                shape
                    .iter()
                    .flat_map(|v| scalar_field(1, *v))
                    .collect::<Vec<_>>(),
                scalar_field(2, dtype),
                message(8, name.as_bytes()),
                message(9, raw),
            ]
            .concat(),
        )
    };
    let node = |op: &str, inputs: &[&str], output: &str| {
        [
            inputs
                .iter()
                .flat_map(|n| message(1, n.as_bytes()))
                .collect::<Vec<_>>(),
            message(2, output.as_bytes()),
            message(4, op.as_bytes()),
        ]
        .concat()
    };
    let mut nodes = message(1, &node("Mul", &["a", "factor"], "computed"));
    let weight = decode(case["weight_pattern_hex"].as_str().unwrap())
        .into_iter()
        .cycle()
        .take(w.iter().product::<usize>() * 2)
        .collect::<Vec<_>>();
    let mut initial = [
        tensor("factor", &[], &1.0001f32.to_le_bytes(), 1),
        tensor("w", &w, &weight, 10),
    ]
    .concat();
    let operand = if transposed || flattened {
        "view"
    } else {
        "computed"
    };
    if transposed {
        let attribute = message(
            5,
            &[
                message(1, b"perm"),
                [0, 2, 1]
                    .into_iter()
                    .flat_map(|v| scalar_field(8, v))
                    .collect::<Vec<_>>(),
                scalar_field(20, 7),
            ]
            .concat(),
        );
        nodes.extend(message(
            1,
            &[node("Transpose", &["computed"], operand), attribute].concat(),
        ));
    }
    if flattened {
        initial.extend(tensor(
            "shape",
            &[2],
            &[matrix[0] as i64, matrix[1] as i64]
                .into_iter()
                .flat_map(i64::to_le_bytes)
                .collect::<Vec<_>>(),
            7,
        ));
        nodes.extend(message(
            1,
            &node("Reshape", &["computed", "shape"], operand),
        ));
    }
    nodes.extend(message(1, &node("MatMul", &[operand, "w"], "output")));
    let mut output_shape = matrix.clone();
    *output_shape.last_mut().unwrap() = w[1];
    let mut outputs = value_info(12, "output", &output_shape, 1);
    if held {
        outputs.extend(value_info(12, operand, &matrix, 1));
        outputs.extend(value_info(12, "a", &x, 1));
    }
    let source = [
        message(8, &scalar_field(2, 20)),
        message(
            7,
            &[nodes, initial, value_info(11, "a", &x, 1), outputs].concat(),
        ),
    ]
    .concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        case["onnx_sha256"].as_str().unwrap()
    );
    source
}
#[test]
fn original_computed_held_and_noncontiguous_matrix_pipeline() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-batched-matmul-controls.json"
    ))
    .unwrap();
    let config = provider();
    for case in fixture["cases"].as_array().unwrap() {
        let source = control_source(case);
        for memory_planning in [false, true] {
            eprintln!("matrix control {} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("model.onnx");
            fs::write(&path, &source).unwrap();
            let destination = temp.path().join("bundle");
            compile_onnx(
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
            )
            .unwrap();
            // SAFETY: locally built immutable artifact and exact declared input bytes.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            for (name, expected) in case["outputs"].as_object().unwrap() {
                let mut output = vec![0; model.output_size(name).unwrap()];
                model.read_output(name, &mut output).unwrap();
                let expected = decode(expected["hex"].as_str().unwrap());
                assert!(output == expected,"{} memory={memory_planning} output={name} firstbyte={:?} actual={:x} expected={:x}",case["name"],output.iter().zip(&expected).position(|(a,b)|a!=b),Sha256::digest(&output),Sha256::digest(&expected));
            }
        }
    }
}

#[test]
fn llvm_matrix_selection_retains_unproved_shared_operand_fences() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-matmul-cpu.json")).unwrap();
    let config = provider();
    for case in fixture["unsupported_aliases"].as_array().unwrap() {
        let source = decode(case["onnx_hex"].as_str().unwrap());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("model.onnx");
        fs::write(&path, &source).unwrap();
        let destination = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&source)),
            &CompileOptions {
                offline_bindings: true,
                llvm: Some((
                    config.clone(),
                    PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                )),
                ..CompileOptions::default()
            },
            &destination,
        );
        if case["name"] == "shared-computed" {
            // Preserve this original case as an exact positive for the newly
            // proved direct residual. Other reuse contexts retain their fences.
            let artifact = result.unwrap();
            assert_eq!(artifact.graph.backend, "cpu-llvm");
            // SAFETY: locally compiled immutable artifact and exact input ABI.
            let mut model = unsafe { CpuModel::load(&destination) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            let mut output = vec![0; model.output_size("output").unwrap()];
            model.read_output("output", &mut output).unwrap();
            assert_eq!(output, decode(case["original_hex"].as_str().unwrap()));
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("alias"), "{}: {error}", case["name"]);
            assert!(!destination.exists());
        }
    }
}

#[test]
fn selected_half_weight_matrix_keeps_shared_alu_ancestor_fenced() {
    use openpilot_model_runtime::{
        compiler_backend_cpu::matrix::graph::closures, compiler_onnx::lower::LoweredGraph,
    };
    let controls: Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-batched-matmul-controls.json"
    ))
    .unwrap();
    let source = control_source(&controls["cases"][0]);
    let added = [
        message(
            1,
            &[
                message(1, b"computed"),
                message(1, b"computed"),
                message(2, b"shared_alu"),
                message(4, b"Add"),
            ]
            .concat(),
        ),
        value_info(12, "shared_alu", &[1, 3, 8], 1),
    ]
    .concat();
    let source = fields(&source)
        .into_iter()
        .flat_map(|(field, payload, raw)| {
            if field == 7 {
                message(7, &[payload, &added].concat())
            } else {
                raw.to_vec()
            }
        })
        .collect::<Vec<_>>();
    let model = Model::parse(&source).unwrap();
    assert_eq!(
        closures(&LoweredGraph::lower_offline(&model).unwrap(), &model)
            .unwrap()
            .len(),
        1
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("model.onnx");
    let destination = temp.path().join("bundle");
    fs::write(&path, &source).unwrap();
    let error = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            llvm: Some((
                provider(),
                PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &destination,
    )
    .unwrap_err();
    assert!(error.contains("inverse movement alias"), "{error}");
    assert!(!destination.exists());
}

#[test]
fn full_driving_unproved_fused_matrix_does_not_publish() {
    let receipt = &fixture()["actual_prefix_receipts"][0];
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/selfdrive/modeld/models")
        .join(receipt["source_model"].as_str().unwrap());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("bundle");
    let error = compile_onnx(
        &path,
        receipt["source_model_sha256"].as_str().unwrap(),
        &CompileOptions {
            offline_bindings: true,
            llvm: Some((
                provider(),
                PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
            )),
            ..CompileOptions::default()
        },
        &destination,
    )
    .unwrap_err();
    assert!(
        error.contains("CPU IMAGE matrix requires f32/half rank-two operands"),
        "{error}"
    );
    assert!(!destination.exists());
}
