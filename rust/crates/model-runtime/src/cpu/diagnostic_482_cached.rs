//! Fresh91 전용 additive C. 기대값은 같은 job B의 기존 원본12출력뿐이다.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedRequest {
    schema: u32,
    triplet_request: Input,
    readonly: Input,
    admission: Input,
    issuer: Input,
    guarded: Input,
    exporter_roster: Input,
    diagnostic: Input,
    traffic: PathBuf,
    bundle: PathBuf,
    bundle_files: BTreeMap<String, Input>,
    files: BTreeMap<String, Input>,
}

#[test]
#[ignore = "GHA A whole fresh91/B12 원문과 독립 issuer admission 필요; C만 명시 cached 실행"]
fn complete_fresh91_cached_context_matches_existing_original() {
    let (root, job) = environment();
    let (request, mut guards) = request(&root, &job);
    roster(&request).unwrap();
    assert_eq!(request.programs.len(), 91);
    let path = PathBuf::from(std::env::var_os("MODEL482_CACHED_REQUEST").unwrap());
    let held = Held::open(
        Input {
            bytes: fs::metadata(&path).unwrap().len() as usize,
            path,
            sha256: std::env::var("MODEL482_CACHED_REQUEST_SHA256").unwrap(),
        },
        4 * MIB,
        &job,
    )
    .unwrap();
    let capture: CachedRequest = serde_json::from_slice(&held.bytes).unwrap();
    assert_eq!(capture.schema, 1);
    assert_eq!(capture.triplet_request.path, guards[0].input.path);
    assert_eq!(capture.triplet_request.sha256, guards[0].input.sha256);
    assert_eq!(capture.traffic, job.join("build/model482/traffic"));
    assert_eq!(
        capture.bundle,
        job.join("build/model482/cached-context/bundle")
    );
    assert_eq!(
        capture.diagnostic.path,
        root.join("rust/crates/model-runtime/src/cpu/diagnostic_482_cached.rs")
    );
    assert_eq!(
        capture.diagnostic.sha256,
        sha(include_bytes!("diagnostic_482_cached.rs"))
    );
    guards.push(Held::open(capture.diagnostic, 32 * MIB, &root).unwrap());
    let readonly = Held::open(capture.readonly, 4 * MIB, &job).unwrap();
    let proof: Value = serde_json::from_slice(&readonly.bytes).unwrap();
    assert_eq!(proof["strict_triplet_all"], true);
    assert_eq!(proof["host_isa_all"], true);
    assert_eq!(proof["source"], request.source_sha256);
    assert_eq!(proof["complete_roster"], 91);
    assert_eq!(proof["programs"].as_array().unwrap().len(), 91);
    let admission_file = Held::open(capture.admission, 4 * MIB, &job).unwrap();
    // Expected identities are controller/A inputs; never read them from the C graph/certificate.
    let admission = cached_code::Admission::read_authenticated(
        &admission_file.input.path,
        &admission_file.input.sha256,
    )
    .unwrap();
    assert_eq!(admission.original_manifest_sha256, guards[0].input.sha256);
    assert_eq!(admission.original_model_sha256, request.model.sha256);
    assert_eq!(admission.programs.len(), 91);
    for seal in admission
        .current
        .guarded_add_sources
        .iter()
        .chain(&admission.current.cached_sources)
    {
        assert_eq!(Some(&seal.sha256), request.sources.get(&seal.path));
    }
    assert_eq!(
        admission.current.isa_verifier_source_sha256,
        request.sources["rust/crates/model-runtime/src/cached_isa.rs"]
    );
    for (input, expected) in [
        (
            capture.issuer,
            admission.current.cached_helper_sha256.as_str(),
        ),
        (capture.guarded, admission.current.helper_sha256.as_str()),
        (
            capture.exporter_roster,
            admission.current.exporter_roster_sha256.as_str(),
        ),
    ] {
        assert_eq!(input.sha256, expected);
        guards.push(Held::open(input, 128 * MIB, &job).unwrap());
    }
    let executable = std::env::current_exe().unwrap();
    guards.push(
        Held::open_mode(
            Input {
                bytes: fs::metadata(&executable).unwrap().len() as usize,
                path: executable,
                sha256: admission.current.isa_verifier_sha256.clone(),
            },
            128 * MIB,
            &job,
            false,
        )
        .unwrap(),
    );
    let mut images = Vec::new();
    for (index, program) in request.programs.iter().enumerate() {
        assert_eq!(program.original, admission.programs[index]);
        let source = Held::open(program.source.clone(), 16 * MIB, &job).unwrap();
        let image = Held::open(program.image.clone(), 16 * MIB, &job).unwrap();
        let object = Held::open(program.object.clone(), 16 * MIB, &job).unwrap();
        cached_code::verify_original_program(
            &program.original,
            &source.bytes,
            &image.bytes,
            &object.bytes,
        )
        .unwrap();
        images.push((image.bytes.clone(), program.original.text_bytes));
        guards.extend([source, image, object]);
    }
    let refs: Vec<_> = images
        .iter()
        .map(|(image, text)| (image.as_slice(), *text))
        .collect();
    let decoded = cached_isa::admit_all(&refs, 91, &cached_isa::current_host()).unwrap();
    for (index, actual) in decoded.iter().enumerate() {
        assert_eq!(proof["programs"][index]["index"], index);
        assert_eq!(proof["programs"][index]["text_bytes"], actual.text_bytes);
        assert_eq!(
            proof["programs"][index]["instructions"],
            actual.instructions
        );
        assert_eq!(proof["programs"][index]["required"], json!(actual.required));
    }
    let mut files = BTreeMap::new();
    let mut materialized = 0;
    for (name, input) in capture.files {
        assert_eq!(input.path, capture.traffic.join(&name));
        let read = name == "sequence.json"
            || name == "bundle/graph.json"
            || name.starts_with("original/")
            || (!name.contains('/') && name.ends_with(".bin"));
        if read {
            materialized += input.bytes;
            assert!(materialized <= 128 * MIB);
        }
        let file = Held::open_mode(input, 512 * MIB, &capture.traffic, read).unwrap();
        if read {
            files.insert(name, file.bytes.clone());
        }
        guards.push(file);
    }
    let mut candidate_files = Vec::new();
    let mut graph_bytes = None;
    for (name, input) in capture.bundle_files {
        assert_eq!(input.path, capture.bundle.join(&name));
        let graph = name == "graph.json";
        let file = Held::open_mode(
            input,
            if graph { 4 * MIB } else { 512 * MIB },
            &capture.bundle,
            graph,
        )
        .unwrap();
        if graph {
            graph_bytes = Some(file.bytes.clone());
        }
        candidate_files.push(file);
    }
    // Candidate graph bytes are not an admission. All state/bindings must match the held B graph.
    let mut candidate: Value = serde_json::from_slice(&graph_bytes.unwrap()).unwrap();
    assert_eq!(candidate["backend"], "cpu-cached-llvm");
    assert_eq!(candidate["version"], 3);
    let mut baseline: Value = serde_json::from_slice(&files["bundle/graph.json"]).unwrap();
    for key in ["backend", "version", "library_sha256", "cached_code"] {
        candidate.as_object_mut().unwrap().remove(key);
        baseline.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(
        candidate, baseline,
        "C changes B initial state/graph binding"
    );
    // Default loading must still reject the candidate before dlopen.
    assert!(unsafe { CpuModel::load(&capture.bundle) }.is_err());
    let mut wrong = admission.clone();
    wrong.original_manifest_sha256 = "0".repeat(64);
    assert!(unsafe { CpuModel::load_cached(&capture.bundle, &wrong) }.is_err());
    // SAFETY: A and the issuer authenticated all91 SOURCE/object/image/ABI/ISA;
    // controller owns immutable candidate files, held below through both frames.
    let mut model = unsafe { CpuModel::load_cached(&capture.bundle, &admission) }.unwrap();
    assert_eq!(model.graph.calls.len(), request.calls.len());
    for (actual, expected) in model.graph.calls.iter().zip(&request.calls) {
        assert_eq!(
            (
                actual.kernel,
                &actual.views,
                &actual.scalars,
                actual.workers,
                actual.core_id
            ),
            (
                expected.kernel,
                &expected.views,
                &expected.scalars,
                expected.workers,
                expected.core_id
            )
        );
    }
    let frames: Vec<Frame> = serde_json::from_slice(&files["sequence.json"]).unwrap();
    assert_eq!(frames.len(), 4);
    let mut outputs = BTreeMap::new();
    let mut mismatches = Vec::new();
    let mut output_bytes = 0;
    let output_directory = capture.bundle.parent().unwrap().join("outputs");
    fs::create_dir(&output_directory).unwrap();
    for (index, frame) in frames.into_iter().enumerate() {
        assert_eq!(
            frame.entrypoint,
            if index % 2 == 0 { "prepare" } else { "policy" }
        );
        for (name, path) in frame.inputs {
            assert_eq!(path.components().count(), 1);
            model
                .write_input(&name, &files[path.to_str().unwrap()])
                .unwrap();
        }
        model.run_entry(&frame.entrypoint).unwrap();
        for (name, path) in frame.outputs {
            assert_eq!(path.components().count(), 1);
            let expected = &files[&format!("original/{}", path.display())];
            assert_eq!(model.output_size(&name).unwrap(), expected.len());
            output_bytes += expected.len();
            assert!(output_bytes <= 64 * MIB);
            let mut actual = vec![0; expected.len()];
            model.read_output(&name, &mut actual).unwrap();
            let mut raw = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(output_directory.join(&path))
                .unwrap();
            raw.write_all(&actual).unwrap();
            if &actual != expected {
                mismatches.push(path.clone());
            }
            outputs.insert(path.to_str().unwrap().to_owned(), json!({"bytes":actual.len(),
                "cached_sha256":sha(&actual),"existing_original_sha256":sha(expected),"equal":&actual==expected}));
        }
    }
    assert_eq!(outputs.len(), 12);
    drop(model);
    guards.extend(candidate_files);
    guards.extend([readonly, admission_file, held]);
    for file in &guards {
        file.verify().unwrap();
    }
    receipt(
        &job,
        "cached-context.json",
        json!({"source":request.source_sha256,"complete_roster":91,
        "diagnostic_complete":true,"frames":2,"outputs":outputs,"mismatches":mismatches,"existing_original_all12_equal":mismatches.is_empty(),
        "cached_issuer_invoked":true,"cached_runtime_enabled":true,"scope":"explicit additive C only",
        "default_cached_disabled":true,"new_original_executions":0,"new_taps":0,"b_failure_can_be_promoted":false,
        "full128_acceptance":false,"full_model_runtime_acceptance":false,"retirement_credit":0}),
    );
    assert!(
        mismatches.is_empty(),
        "C differs from existing B original whole outputs"
    );
}
