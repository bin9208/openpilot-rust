//! PR1344 전용 private test leaf. production 및 기존 road1928 두 frame 관측은 불변이다.
use super::*;

fn own_source(root: &Path) -> Held {
    assert_eq!(std::env::var("GITHUB_WORKFLOW_REF").unwrap(),
        "bin9208/openpilot-rust/.github/workflows/python-rust-523-model-pr1344-replay.yml@refs/heads/codex/feat-523-python-rust");
    let input = Input {
        path: root.join("rust/crates/model-runtime/src/cpu/diagnostic_482_pr1344.rs"),
        bytes: include_bytes!("diagnostic_482_pr1344.rs").len(),
        sha256: sha(include_bytes!("diagnostic_482_pr1344.rs")),
    };
    assert_eq!(
        input.sha256,
        std::env::var("MODEL482_NATIVE_TEST_SHA256").unwrap()
    );
    Held::open(input, MIB, root).unwrap()
}

#[test]
#[ignore = "등록 Actions의 sealed frozen PKL/source 및 fresh whole object/image가 필요"]
fn frozen_source_whole_triplets_before_runtime() {
    let (root, job) = environment();
    let source = own_source(&root);
    fresh_program_roster_triplets_and_host_isa_are_bound_without_execution();
    source.verify().unwrap();
    receipt(
        &job,
        "pr1344-module.json",
        json!({"source":std::env::var("GITHUB_SHA").unwrap(),
        "module_sha256":source.input.sha256,"whole_A_reused":true,"runtime_executed":false}),
    );
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replay {
    schema: u32,
    triplet_request: Input,
    readonly: Input,
    traffic: PathBuf,
    bundle: PathBuf,
    files: BTreeMap<String, Input>,
    bundle_files: BTreeMap<String, Input>,
}
struct State {
    function: KernelFn,
    address: usize,
    events: Vec<(usize, [u8; 4], [u8; 4], [usize; 10])>,
    error: bool,
}
thread_local! { static WRITER: RefCell<Option<State>> = const { RefCell::new(None) }; }

// 동일 함수/ABI를 한 번 forward한다. callback에서 panic/unwind/JSON/동기화 없음.
unsafe extern "C" fn writer99(buffers: *const *mut c_void, scalars: *const i32) {
    let state = WRITER.try_with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return None;
        };
        let state = guard.as_mut()?;
        if buffers.is_null() || scalars.is_null() || state.events.len() >= 2 {
            state.error = true;
            return None;
        }
        // SAFETY: 등록 위치는 검증된10pointer/1scalar,2worker call 한 곳이다.
        let worker = unsafe { *scalars };
        let mut pointers = [0usize; 10];
        for (i, target) in pointers.iter_mut().enumerate() {
            *target = unsafe { *buffers.add(i) } as usize;
        }
        if !(0..2).contains(&worker) || pointers[0] != state.address {
            state.error = true;
            return None;
        }
        let mut before = [0u8; 4];
        if worker == 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (state.address + 99 * 4) as *const u8,
                    before.as_mut_ptr(),
                    4,
                )
            };
        }
        Some((
            state.function,
            state.address,
            worker as usize,
            before,
            pointers,
        ))
    });
    let Ok(Some((function, address, worker, before, pointers))) = state else {
        return;
    };
    // SAFETY: 실제 validated invocation의 인자와 동일하며 model이 library/buffer를 보유한다.
    unsafe { function(buffers, scalars) };
    let mut after = [0u8; 4];
    if worker == 0 {
        unsafe {
            std::ptr::copy_nonoverlapping((address + 99 * 4) as *const u8, after.as_mut_ptr(), 4)
        };
    }
    let _ = WRITER.try_with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                state.events.push((worker, before, after, pointers));
            }
        }
    });
}

#[test]
#[ignore = "same-job A와 D1의 current catalog 이후 private 최초3stage plain/tapped 전용"]
fn first_packet_whole_outputs_and_writer99() {
    let (root, job) = environment();
    let source = own_source(&root);
    let mode = std::env::var("MODEL482_REPLAY_MODE").unwrap();
    assert!(matches!(mode.as_str(), "plain" | "tapped"));
    let tapped = mode == "tapped";
    let (request, mut guards) = request(&root, &job);
    roster(&request).unwrap();
    let path = PathBuf::from(std::env::var_os("MODEL482_REPLAY_REQUEST").unwrap());
    let held = Held::open(
        Input {
            bytes: fs::metadata(&path).unwrap().len() as usize,
            path,
            sha256: std::env::var("MODEL482_REPLAY_REQUEST_SHA256").unwrap(),
        },
        4 * MIB,
        &job,
    )
    .unwrap();
    let capture: Replay = serde_json::from_slice(&held.bytes).unwrap();
    assert_eq!(capture.schema, 1);
    assert_eq!(capture.triplet_request.sha256, guards[0].input.sha256);
    assert_eq!(capture.triplet_request.path, guards[0].input.path);
    let readonly = Held::open(capture.readonly, 4 * MIB, &job).unwrap();
    let proof: Value = serde_json::from_slice(&readonly.bytes).unwrap();
    assert_eq!(proof["strict_triplet_all"], true);
    assert_eq!(proof["host_isa_all"], true);
    assert_eq!(proof["source"], request.source_sha256);
    assert_eq!(proof["complete_roster"], request.programs.len());
    guards.extend([held, readonly, source]);
    assert_eq!(capture.traffic, job.join("build/model482/traffic"));
    let catalog = job.join("build/model482/current-catalog");
    let pointer: Value =
        serde_json::from_slice(&fs::read(catalog.join("current.json")).unwrap()).unwrap();
    let generation = pointer["generation"].as_str().unwrap();
    assert_eq!(generation.len(), 64);
    assert!(generation.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        capture.bundle,
        catalog.join(generation).join("driving-1344x760")
    );
    let mut files = BTreeMap::new();
    let mut kernel_sources = BTreeMap::new();
    for (name, input) in capture.files {
        assert_eq!(input.path, capture.traffic.join(&name));
        let held = Held::open(input, 16 * MIB, &capture.traffic).unwrap();
        files.insert(name, held.bytes.clone());
        guards.push(held);
    }
    for (name, input) in capture.bundle_files {
        assert_eq!(Path::new(&name).components().count(), 1);
        assert_eq!(input.path, capture.bundle.join(&name));
        kernel_sources.insert(name.clone(), input.sha256.clone());
        guards.push(Held::open_mode(input, 512 * MIB, &capture.bundle, false).unwrap());
    }
    assert!(guards
        .iter()
        .any(|g| g.input.path == capture.bundle.join("graph.json")));
    assert!(guards
        .iter()
        .any(|g| g.input.path == capture.bundle.join("weights.bin")));
    assert!(guards
        .iter()
        .any(|g| g.input.path == capture.bundle.join("kernels.so")));
    let selected: Vec<_> = request
        .programs
        .iter()
        .filter(|p| p.original.source_sha256 == STORE_SOURCE)
        .collect();
    assert_eq!(
        selected.len(),
        1,
        "exact authenticated writer source required"
    );
    for program in &request.programs {
        assert_eq!(
            kernel_sources[&format!("kernel-{}.txt", program.kernel)],
            program.original.source_sha256
        );
    }
    // SAFETY: A 전체 image/hostISA 통과 뒤 같은 job의 current catalog를 FD로 보유한다.
    let mut model = unsafe { CpuModel::load(&capture.bundle) }.unwrap();
    assert!(model.graph.cached_code.is_none());
    let entries: Value = serde_json::from_slice(&files["entrypoints.json"]).unwrap();
    assert_eq!(
        serde_json::to_value(&model.graph.entrypoints).unwrap(),
        entries
    );
    assert_eq!(
        model
            .graph
            .inputs
            .iter()
            .map(|b| (b.name.clone(), b.view))
            .collect::<BTreeMap<_, _>>(),
        request.inputs
    );
    assert_eq!(
        model
            .graph
            .outputs
            .iter()
            .map(|b| (b.name.clone(), b.view))
            .collect::<BTreeMap<_, _>>(),
        request.outputs
    );
    assert_eq!(model.graph.calls.len(), request.calls.len());
    for (actual, expected) in model.graph.calls.iter().zip(&request.calls) {
        assert_eq!(
            (actual.kernel, actual.workers, actual.core_id),
            (expected.kernel, expected.workers, expected.core_id)
        );
        assert_eq!(actual.views, expected.views);
        assert_eq!(actual.scalars, expected.scalars);
    }
    assert_eq!(model.graph.views.len(), request.views.len());
    for (a, e) in model.graph.views.iter().zip(&request.views) {
        assert_eq!(
            (a.allocation, a.offset, a.bytes),
            (e.allocation, e.offset, e.bytes)
        );
    }
    assert_eq!(
        model
            .graph
            .allocations
            .iter()
            .map(|a| a.bytes)
            .collect::<Vec<_>>(),
        request.allocations
    );
    let initial: Vec<Value> = serde_json::from_slice(&files["initial.json"]).unwrap();
    assert_eq!(initial.len(), model.buffers.len());
    for (index, (buffer, expected)) in model.buffers.iter().zip(&initial).enumerate() {
        let mut bytes = vec![0; request.allocations[index]];
        buffer.read(0, &mut bytes).unwrap();
        assert_eq!(expected["bytes"], bytes.len());
        assert_eq!(
            expected["sha256"],
            sha(&bytes),
            "initial allocation{index}; missing provenance is HOLD"
        );
    }
    let indices: Vec<_> = request
        .calls
        .iter()
        .filter(|c| c.program == Some(selected[0].index))
        .map(|c| c.index)
        .collect();
    assert_eq!(indices.len(), 1);
    let call_index = indices[0];
    let output = &model.graph.views[request.outputs["model"]];
    assert_eq!((output.offset, output.bytes), (0, 10304));
    let call = &model.calls[call_index];
    assert_eq!(
        (
            call.buffers.len(),
            call.scalars.len(),
            call.workers,
            call.core_id
        ),
        (10, 1, 2, Some(0))
    );
    let address = model.buffers[output.allocation]
        .view_ptr(0, output.bytes)
        .unwrap() as usize;
    assert_eq!(call.buffers[0] as usize, address);
    for (i, other) in model.graph.calls.iter().enumerate() {
        if i != call_index {
            assert!(
                !other
                    .views
                    .iter()
                    .any(|v| model.graph.views[*v].allocation == output.allocation),
                "unresolved other writer/alias"
            );
        }
    }
    let function = model.calls[call_index].function;
    if tapped {
        WRITER.with(|cell| {
            *cell.borrow_mut() = Some(State {
                function,
                address,
                events: Vec::with_capacity(2),
                error: false,
            })
        });
        // 필수 인자 부재 거부는 real kernel 실행 전 확인한다.
        unsafe { writer99(std::ptr::null(), std::ptr::null()) };
        WRITER.with(|cell| {
            let mut state = cell.borrow_mut();
            let state = state.as_mut().unwrap();
            assert!(state.error && state.events.is_empty());
            state.error = false;
        });
        model.calls[call_index].function = writer99;
    }
    let frames: Vec<Frame> = serde_json::from_slice(&files["sequence.json"]).unwrap();
    assert_eq!(frames.len(), 3);
    let mut outputs = BTreeMap::new();
    for (index, frame) in frames.into_iter().enumerate() {
        assert_eq!(
            frame.entrypoint,
            if index < 2 { "prepare" } else { "policy" }
        );
        if index < 2 {
            assert_eq!(
                frame
                    .inputs
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from(["frame", "big_frame", "tfm", "big_tfm", "packed_npy_inputs"])
            );
            assert!(frame.outputs.is_empty());
        } else {
            assert!(frame.inputs.is_empty());
            assert_eq!(
                frame.outputs.keys().collect::<BTreeSet<_>>(),
                request.outputs.keys().collect::<BTreeSet<_>>()
            );
        }
        for (name, path) in frame.inputs {
            assert_eq!(path.components().count(), 1);
            assert_eq!(path.to_str().unwrap(), format!("{name}-{index}.bin"));
            model
                .write_input(&name, &files[path.to_str().unwrap()])
                .unwrap();
        }
        model.run_entry(&frame.entrypoint).unwrap();
        for (name, path) in frame.outputs {
            assert_eq!(path.components().count(), 1);
            let filename = path.to_str().unwrap();
            assert_eq!(filename, format!("{name}-0.bin"));
            let mut bytes = vec![0; model.output_size(&name).unwrap()];
            model.read_output(&name, &mut bytes).unwrap();
            if tapped {
                assert_eq!(
                    bytes, files[filename],
                    "native whole-output plain/tapped drift"
                );
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(capture.traffic.join(filename))
                    .unwrap();
                file.write_all(&bytes).unwrap();
            }
            outputs.insert(name, json!({"bytes":bytes.len(),"sha256":sha(&bytes)}));
        }
    }
    model.calls[call_index].function = function;
    let mut events = Vec::new();
    if tapped {
        let state = WRITER.with(|cell| cell.borrow_mut().take().unwrap());
        assert!(!state.error);
        assert_eq!(state.events.iter().map(|e| e.0).collect::<Vec<_>>(), [0, 1]);
        events=state.events.iter().map(|(worker,before,after,pointers)|json!({"worker":worker,"index":if *worker==0 {Some(99)}else{None},
            "before_le_bytes":if *worker==0 {Some(before)}else{None},"after_le_bytes":if *worker==0 {Some(after)}else{None},"pointer_args":pointers})).collect();
    }
    for held in &guards {
        held.verify().unwrap();
    }
    receipt(
        &job,
        &format!("native-{mode}.json"),
        json!({"source":request.source_sha256,"outputs":outputs,"events":events,
        "initial_allocation_bytes_equal":true,"plain_tapped_all_outputs_equal":tapped,"writer_call_observed":tapped,
        "ssa_first_producer_observed":false,"historical_half_snan5_resolved":false,"full128_acceptance":false}),
    );
}
