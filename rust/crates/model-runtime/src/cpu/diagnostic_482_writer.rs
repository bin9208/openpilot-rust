//! D 전용: C의 원본91 admission 이후 test 안에서 writer 호출 하나만 교체한다.
use super::*;
use crate::cached_elf;
use std::os::fd::AsRawFd;
use std::os::unix::fs::DirBuilderExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    source: Input,
    object: Input,
    library: Option<Input>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriterRequest {
    schema: u32,
    cached_request: Input,
    baseline_receipt: Input,
    diagnostic: Input,
    compile_request: Input,
    compiler_receipt: Input,
    variants: BTreeMap<String, Variant>,
}

// 기존 C request를 독립적으로 읽는다. 새 writer를 C certificate에 넣지 않는다.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
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

struct Candidate {
    image: Vec<u8>,
    text_bytes: usize,
    isa: cached_isa::IsaProof,
    package: Vec<u8>,
    library: Option<Held>,
}

struct Context {
    job: PathBuf,
    directory: PathBuf,
    bundle: PathBuf,
    request: Request,
    admission: cached_code::Admission,
    guards: Vec<Held>,
    files: BTreeMap<String, Vec<u8>>,
    candidates: BTreeMap<String, Candidate>,
    compiler: Value,
    call: usize,
}

fn hold(input: &Input, limit: usize, root: &Path, guards: &mut Vec<Held>, read: bool) -> Vec<u8> {
    let held = Held::open_mode(input.clone(), limit, root, read).unwrap();
    let bytes = held.bytes.clone();
    guards.push(held);
    bytes
}

fn load() -> Context {
    let (root, job) = environment();
    let (request, mut guards) = request(&root, &job);
    roster(&request).unwrap();
    assert_eq!(request.programs.len(), 91);
    let directory = job.join("build/model482/cached-context/writer-pair");
    let path = PathBuf::from(std::env::var_os("MODEL482_WRITER_REQUEST").unwrap());
    assert!(matches!(
        path.file_name().unwrap().to_str().unwrap(),
        "prepare-request.json" | "native-request.json"
    ));
    assert_eq!(path.parent(), Some(directory.as_path()));
    let input = Input {
        bytes: fs::metadata(&path).unwrap().len() as usize,
        path,
        sha256: std::env::var("MODEL482_WRITER_REQUEST_SHA256").unwrap(),
    };
    let bytes = hold(&input, 4 * MIB, &directory, &mut guards, true);
    let d: WriterRequest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(d.schema, 1);
    assert_eq!(
        d.variants.keys().map(String::as_str).collect::<Vec<_>>(),
        ["guarded", "rename-only"]
    );
    assert_eq!(
        d.diagnostic.path,
        root.join("rust/crates/model-runtime/src/cpu/diagnostic_482_writer.rs")
    );
    assert_eq!(
        d.diagnostic.sha256,
        sha(include_bytes!("diagnostic_482_writer.rs"))
    );
    hold(&d.diagnostic, 32 * MIB, &root, &mut guards, false);
    assert_eq!(
        d.cached_request.path,
        directory.parent().unwrap().join("native-request.json")
    );
    let bytes = hold(&d.cached_request, 4 * MIB, &job, &mut guards, true);
    let baseline: Baseline = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(baseline.schema, 1);
    assert_eq!(baseline.triplet_request.path, guards[0].input.path);
    assert_eq!(baseline.triplet_request.sha256, guards[0].input.sha256);
    assert_eq!(baseline.traffic, job.join("build/model482/traffic"));
    assert_eq!(
        baseline.bundle,
        job.join("build/model482/cached-context/bundle")
    );
    assert_eq!(
        baseline.diagnostic.path,
        root.join("rust/crates/model-runtime/src/cpu/diagnostic_482_cached.rs")
    );
    assert_eq!(
        baseline.diagnostic.sha256,
        sha(include_bytes!("diagnostic_482_cached.rs"))
    );
    hold(&baseline.diagnostic, 32 * MIB, &root, &mut guards, false);
    assert_eq!(
        d.baseline_receipt.path,
        job.join("proof/model482/cached-context.json")
    );
    let bytes = hold(&d.baseline_receipt, 4 * MIB, &job, &mut guards, true);
    let c: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(c["source"], request.source_sha256);
    assert_eq!(c["existing_original_all12_equal"], true);
    assert_eq!(c["diagnostic_complete"], true);
    assert_eq!(c["default_cached_disabled"], true);
    let bytes = hold(&baseline.readonly, 4 * MIB, &job, &mut guards, true);
    let proof: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(proof["source"], request.source_sha256);
    assert_eq!(proof["complete_roster"], 91);
    assert_eq!(proof["strict_triplet_all"], true);
    assert_eq!(proof["host_isa_all"], true);
    hold(&baseline.admission, 4 * MIB, &job, &mut guards, false);
    let admission = cached_code::Admission::read_authenticated(
        &baseline.admission.path,
        &baseline.admission.sha256,
    )
    .unwrap();
    assert_eq!(admission.original_manifest_sha256, guards[0].input.sha256);
    assert_eq!(admission.original_model_sha256, request.model.sha256);
    assert_eq!(admission.programs.len(), 91);
    hold(&request.model, 128 * MIB, &job, &mut guards, false);
    for seal in admission
        .current
        .guarded_add_sources
        .iter()
        .chain(&admission.current.cached_sources)
    {
        assert_eq!(request.sources.get(&seal.path), Some(&seal.sha256));
    }
    assert_eq!(
        admission.current.isa_verifier_source_sha256,
        request.sources["rust/crates/model-runtime/src/cached_isa.rs"]
    );
    for (input, expected) in [
        (&baseline.issuer, &admission.current.cached_helper_sha256),
        (&baseline.guarded, &admission.current.helper_sha256),
        (
            &baseline.exporter_roster,
            &admission.current.exporter_roster_sha256,
        ),
    ] {
        assert_eq!(&input.sha256, expected);
        hold(input, 128 * MIB, &job, &mut guards, false);
    }
    let executable = std::env::current_exe().unwrap();
    hold(
        &Input {
            bytes: fs::metadata(&executable).unwrap().len() as usize,
            path: executable,
            sha256: admission.current.isa_verifier_sha256.clone(),
        },
        128 * MIB,
        &job,
        &mut guards,
        false,
    );
    let mut original_images = Vec::new();
    for (index, program) in request.programs.iter().enumerate() {
        assert_eq!(program.original, admission.programs[index]);
        let source = hold(&program.source, 16 * MIB, &job, &mut guards, true);
        let image = hold(&program.image, 16 * MIB, &job, &mut guards, true);
        let object = hold(&program.object, 16 * MIB, &job, &mut guards, true);
        cached_code::verify_original_program(&program.original, &source, &image, &object).unwrap();
        original_images.push((image, program.original.text_bytes));
    }
    let refs: Vec<_> = original_images
        .iter()
        .map(|(b, t)| (b.as_slice(), *t))
        .collect();
    let decoded = cached_isa::admit_all(&refs, 91, &cached_isa::current_host()).unwrap();
    assert_eq!(proof["programs"].as_array().unwrap().len(), 91);
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
    let mut total = 0;
    for (name, input) in &baseline.files {
        assert_eq!(input.path, baseline.traffic.join(name));
        let read = name == "sequence.json"
            || name == "bundle/graph.json"
            || name.starts_with("original/")
            || (!name.contains('/') && name.ends_with(".bin"));
        if read {
            total += input.bytes;
            assert!(total <= 128 * MIB);
        }
        let bytes = hold(input, 512 * MIB, &baseline.traffic, &mut guards, read);
        if read {
            files.insert(name.clone(), bytes);
        }
    }
    let mut graph_bytes = None;
    for (name, input) in &baseline.bundle_files {
        assert_eq!(input.path, baseline.bundle.join(name));
        let bytes = hold(
            input,
            if name == "graph.json" {
                4 * MIB
            } else {
                512 * MIB
            },
            &baseline.bundle,
            &mut guards,
            name == "graph.json",
        );
        if name == "graph.json" {
            graph_bytes = Some(bytes);
        }
    }
    let mut graph: Value = serde_json::from_slice(&graph_bytes.unwrap()).unwrap();
    assert_eq!(graph["backend"], "cpu-cached-llvm");
    assert_eq!(graph["version"], 3);
    let typed_graph = Graph::parse(&serde_json::to_vec(&graph).unwrap())
        .unwrap()
        .graph;
    let mut old_graph: Value = serde_json::from_slice(&files["bundle/graph.json"]).unwrap();
    for key in ["backend", "version", "library_sha256", "cached_code"] {
        graph.as_object_mut().unwrap().remove(key);
        old_graph.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(graph, old_graph);
    // Production의 full91 certificate/ISA/source/current binding을 그대로 적용한다.
    let strict = cached_code::prepare(&baseline.bundle, &typed_graph, &admission).unwrap();
    strict.verify().unwrap();
    let mut wrong = admission.clone();
    wrong.original_manifest_sha256 = "0".repeat(64);
    assert!(cached_code::prepare(&baseline.bundle, &typed_graph, &wrong).is_err());
    let selected: Vec<_> = request
        .programs
        .iter()
        .filter(|p| p.original.source_sha256 == STORE_SOURCE)
        .collect();
    assert_eq!(selected.len(), 1);
    let selected = selected[0];
    assert_eq!(selected.kernel, 92);
    assert_eq!(
        (
            selected.original.pointers,
            selected.original.scalars,
            selected.original.workers,
            selected.original.core_id
        ),
        (10, 1, 2, Some(0))
    );
    assert_eq!(
        selected.original.global_slots,
        (0_usize..10).collect::<Vec<_>>()
    );
    let calls: Vec<_> = request
        .calls
        .iter()
        .filter(|c| c.program == Some(selected.index))
        .collect();
    assert_eq!(calls.len(), 1);
    let call = calls[0].index;
    assert_eq!(request.calls[call].views[0], request.outputs["model"]);
    let original = hold(&selected.source, 16 * MIB, &job, &mut guards, true);
    let text = std::str::from_utf8(&original).unwrap();
    let old_name = text
        .lines()
        .next()
        .unwrap()
        .strip_prefix("define void @")
        .unwrap()
        .split_once('(')
        .unwrap()
        .0;
    assert!(old_name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    assert_eq!(text.matches(&format!("@{old_name}(")).count(), 1);
    let renamed = text.replacen(&format!("@{old_name}("), "@op_impl_92(", 1);
    let canonical = crate::guarded_add::canonicalize(&renamed).unwrap();
    assert_eq!(canonical.receipt.rewrite_count, 4);
    let b_source = &baseline.files["bundle/kernel-92.compiled.ll"];
    assert_eq!(
        renamed.as_bytes(),
        hold(b_source, 16 * MIB, &job, &mut guards, true).as_slice()
    );
    let b_policy = &baseline.files["bundle/kernel-92.source-policy.json"];
    let policy = hold(b_policy, 4 * MIB, &job, &mut guards, true);
    assert_eq!(
        serde_json::from_slice::<Value>(&policy).unwrap(),
        json!({"version":1,"policy":"preserve-original-flags","symbol_rename_only":true,
            "math_rewrite_applied":false,"helper_executed":false,
            "original_source_sha256":selected.source.sha256,
            "renamed_source_sha256":sha(renamed.as_bytes()),
            "compiled_source_sha256":sha(renamed.as_bytes()),
            "original_source_bytes":original.len(),"compiled_source_bytes":renamed.len()})
    );
    let bytes = hold(
        &baseline.files["bundle/compilation-evidence.json"],
        4 * MIB,
        &job,
        &mut guards,
        true,
    );
    let evidence: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(evidence["version"], 1);
    let binding = &evidence["kernels"][selected.kernel.to_string()];
    assert_eq!(binding["source_policy"], "preserve-original-flags");
    assert_eq!(binding["source_policy_receipt_sha256"], b_policy.sha256);
    assert_eq!(binding["original_source_sha256"], selected.source.sha256);
    assert_eq!(binding["renamed_source_sha256"], b_source.sha256);
    assert_eq!(binding["compiled_source_sha256"], b_source.sha256);
    assert_eq!(
        binding["object_sha256"],
        baseline.files["bundle/kernel-92.o"].sha256
    );
    assert_eq!(binding["helper"]["enabled"], true);
    assert_eq!(binding["helper"]["sha256"], admission.current.helper_sha256);
    assert!(binding.get("rewrite_receipt_sha256").is_none());
    assert_eq!(
        d.compile_request.path,
        directory.join("compile-request.json")
    );
    let bytes = hold(&d.compile_request, 4 * MIB, &directory, &mut guards, true);
    let compile_request: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(compile_request["schema"], 1);
    assert_eq!(compile_request["source"], request.source_sha256);
    assert_eq!(
        compile_request["files"]["original"]["sha256"],
        selected.source.sha256
    );
    assert_eq!(
        compile_request["files"]["parity"]["sha256"],
        b_source.sha256
    );
    assert_eq!(
        compile_request["files"]["policy"]["sha256"],
        b_policy.sha256
    );
    let bytes = hold(&request.events, 16 * MIB, &job, &mut guards, true);
    let events: Value = serde_json::from_slice(&bytes).unwrap();
    let expected_config = &events["chains"][selected.compile_event]["config"];
    assert_eq!(
        events["chains"][selected.compile_event]["source"]["sha256"],
        selected.source.sha256
    );
    assert_eq!(
        events["chains"][selected.compile_event]["object"]["sha256"],
        selected.object.sha256
    );
    assert_eq!(
        events["chains"][selected.compile_event]["image"]["sha256"],
        selected.image.sha256
    );
    assert_eq!(
        d.compiler_receipt.path,
        directory.join("compiler-receipt.json")
    );
    let bytes = hold(&d.compiler_receipt, 4 * MIB, &directory, &mut guards, true);
    let compiler: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(compiler["schema"], 1);
    assert_eq!(compiler["variants"].as_object().unwrap().len(), 2);
    assert_eq!(compiler["source"], request.source_sha256);
    assert_eq!(compiler["original_source_sha256"], STORE_SOURCE);
    assert_eq!(compiler["fresh_compiles"], 2);
    assert_eq!(compiler["original_model_executions"], 0);
    assert_eq!(compiler["original_compiler_invocations"], 2);
    for key in ["compile_cached_calls", "original_executions", "taps"] {
        assert_eq!(compiler[key], 0);
    }
    let strict_receipt: Input =
        serde_json::from_value(compiler["guarded_rewrite"].clone()).unwrap();
    assert_eq!(strict_receipt.path, directory.join("guarded-add.json"));
    let bytes = hold(&strict_receipt, 4 * MIB, &directory, &mut guards, true);
    assert_eq!(
        serde_json::to_value(&canonical.receipt).unwrap(),
        serde_json::from_slice::<Value>(&bytes).unwrap()
    );
    let mut candidates = BTreeMap::new();
    for (name, variant) in &d.variants {
        assert_eq!(variant.source.path, directory.join(format!("{name}.ll")));
        assert_eq!(variant.object.path, directory.join(format!("{name}.o")));
        let source = hold(&variant.source, 16 * MIB, &directory, &mut guards, true);
        assert_eq!(
            source.as_slice(),
            if name == "rename-only" {
                renamed.as_bytes()
            } else {
                canonical.source.as_bytes()
            }
        );
        let row = &compiler["variants"][name];
        assert_eq!(row["source"]["sha256"], variant.source.sha256);
        assert_eq!(row["object"]["sha256"], variant.object.sha256);
        assert_eq!(&row["before"], expected_config);
        assert_eq!(row["before"], row["after"]);
        assert_eq!(
            row["context"],
            compiler["variants"]["rename-only"]["context"]
        );
        for key in ["llvm_context", "target_machine", "pass_builder_options"] {
            assert!(row["context"][key].as_u64().unwrap() > 0);
        }
        // pinned autogen llvm.py의 enum 값. 지정 변경 없이 실제 호출 인자를 검사한다.
        assert_eq!(
            row["context"]["constructor_options"],
            json!({"codegen_level":2,"relocation_mode":2,"code_model":0})
        );
        let object = hold(&variant.object, 16 * MIB, &directory, &mut guards, true);
        let reconstructed = cached_elf::reconstruct(&object).unwrap();
        let refs = [(reconstructed.image.as_slice(), reconstructed.text_bytes)];
        let isa = cached_isa::admit_all(&refs, 1, &cached_isa::current_host())
            .unwrap()
            .remove(0);
        for required in &isa.required {
            let mut missing = cached_isa::current_host();
            assert!(missing.remove(required));
            assert!(cached_isa::admit_all(&refs, 1, &missing).is_err());
        }
        let package = cached_elf::package(&[(92, &reconstructed.image)]).unwrap();
        let library = variant.library.as_ref().map(|input| {
            assert_eq!(input.path, directory.join(format!("{name}.so")));
            Held::open(input.clone(), 64 * MIB, &directory).unwrap()
        });
        candidates.insert(
            name.clone(),
            Candidate {
                image: reconstructed.image,
                text_bytes: reconstructed.text_bytes,
                isa,
                package,
                library,
            },
        );
    }
    let context = Context {
        job,
        directory,
        bundle: baseline.bundle,
        request,
        admission,
        guards,
        files,
        candidates,
        compiler,
        call,
    };
    context.verify();
    context
}

impl Context {
    fn verify(&self) {
        for held in &self.guards {
            held.verify().unwrap();
        }
        for candidate in self.candidates.values() {
            if let Some(held) = &candidate.library {
                held.verify().unwrap();
            }
        }
    }
}

#[test]
#[ignore = "GHA C full91/12 성공과 자연 LLVM fresh writer 두 후보 필요; 생성 code 실행 없음"]
fn fresh_writer_pair_is_bound_without_execution() {
    let context = load();
    let mut rows = BTreeMap::new();
    for (name, candidate) in &context.candidates {
        assert!(candidate.library.is_none());
        for (suffix, bytes) in [
            ("container.o", &candidate.package),
            ("bin", &candidate.image),
        ] {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(context.directory.join(format!("{name}.{suffix}")))
                .unwrap();
            file.write_all(bytes).unwrap();
        }
        rows.insert(name, json!({"image_sha256":sha(&candidate.image),"text_bytes":candidate.text_bytes,
            "instructions":candidate.isa.instructions,"required":candidate.isa.required,"package_sha256":sha(&candidate.package)}));
    }
    context.verify();
    receipt(
        &context.job,
        "writer-pair-preflight.json",
        json!({"source":context.request.source_sha256,
        "variants":rows,"complete_original_roster":91,"retained_original_programs":90,"generated_code_executed":false,
        "host_features":cached_isa::current_host(),"candidate_original_certificate_claim":false,
        "production_admission_changed":false,"numerical_acceptance":false}),
    );
}

type WriterFn = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    i32,
);

// -nostdlib로 순수 packed image만 link한다. dlopen의 추가 실행/의존성 경로는 거부한다.
fn passive_library(bytes: &[u8]) -> Result<(), String> {
    fn word(bytes: &[u8], offset: usize) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            bytes
                .get(offset..offset + 8)
                .ok_or("D ELF word")?
                .try_into()
                .unwrap(),
        ))
    }
    if bytes.len() < 64 || &bytes[..7] != b"\x7fELF\x02\x01\x01" {
        return Err("D ELF header".into());
    }
    let offset = usize::try_from(word(bytes, 32)?).map_err(|_| "D phoff")?;
    let width = u16::from_le_bytes(bytes[54..56].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(bytes[56..58].try_into().unwrap()) as usize;
    if width != 56 || !(1..=128).contains(&count) {
        return Err("D program headers".into());
    }
    let mut dynamic = 0;
    for index in 0..count {
        let at = offset
            .checked_add(index * width)
            .ok_or("D program overflow")?;
        let p = bytes
            .get(at..at.checked_add(width).ok_or("D program overflow")?)
            .ok_or("D program range")?;
        let kind = u32::from_le_bytes(p[..4].try_into().unwrap());
        let flags = u32::from_le_bytes(p[4..8].try_into().unwrap());
        if matches!(kind, 3 | 7) || (kind == 1 && flags & 3 == 3) {
            return Err("D interpreter/TLS/RWX".into());
        }
        if kind != 2 {
            continue;
        }
        dynamic += 1;
        let start = usize::try_from(word(p, 8)?).map_err(|_| "D dynamic offset")?;
        let size = usize::try_from(word(p, 32)?).map_err(|_| "D dynamic size")?;
        if size == 0 || !size.is_multiple_of(16) {
            return Err("D dynamic width".into());
        }
        let entries = bytes
            .get(start..start.checked_add(size).ok_or("D dynamic overflow")?)
            .ok_or("D dynamic range")?;
        let mut ended = false;
        for row in entries.chunks_exact(16) {
            let tag = word(row, 0)?;
            let value = word(row, 8)?;
            if ended && (tag != 0 || value != 0) {
                return Err("D dynamic trailing data".into());
            }
            if matches!(tag, 1 | 12 | 13 | 22 | 25 | 26 | 27 | 28 | 32 | 33)
                || (tag == 30 && value & 4 != 0)
            {
                return Err("D dependencies/initializers/textrel".into());
            }
            if tag == 0 {
                if value != 0 {
                    return Err("D NULL value".into());
                }
                ended = true;
            }
        }
        if !ended {
            return Err("D dynamic terminator".into());
        }
    }
    if dynamic != 1 {
        return Err("D dynamic count".into());
    }
    Ok(())
}

thread_local! { static WRITER: std::cell::Cell<Option<WriterFn>> = const { std::cell::Cell::new(None) }; }
struct Dispatch;
impl Drop for Dispatch {
    fn drop(&mut self) {
        WRITER.with(|cell| cell.set(None));
    }
}

// 검증한10pointer/i32 ABI를 기존 KernelFn에 연결하는 test 전용 adapter. 값 관측은 없다.
unsafe extern "C" fn dispatch(buffers: *const *mut c_void, scalars: *const i32) {
    if buffers.is_null() || scalars.is_null() {
        std::process::abort();
    }
    WRITER.with(|cell| {
        let Some(function) = cell.get() else {
            std::process::abort()
        };
        // SAFETY: full91 ABI, selected call arity and live model buffers are checked before replacement.
        unsafe {
            function(
                *buffers,
                *buffers.add(1),
                *buffers.add(2),
                *buffers.add(3),
                *buffers.add(4),
                *buffers.add(5),
                *buffers.add(6),
                *buffers.add(7),
                *buffers.add(8),
                *buffers.add(9),
                *scalars,
            )
        };
    });
}

#[test]
#[ignore = "GHA C full91 strict admission 뒤 test에서만 writer92 한 호출 교체; 기존 원본12 비교"]
fn fresh_writer_pair_reuses_admitted_original90() {
    let context = load();
    let mut variants = BTreeMap::new();
    let mut output_total = 0;
    let output_root = context.directory.join("outputs");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&output_root)
        .unwrap();
    for (name, candidate) in &context.candidates {
        let held = candidate.library.as_ref().unwrap();
        passive_library(&held.bytes).unwrap();
        let mut bad_header = held.bytes.clone();
        let phoff = u64::from_le_bytes(bad_header[32..40].try_into().unwrap()) as usize;
        bad_header[phoff..phoff + 4].copy_from_slice(&3_u32.to_le_bytes());
        assert!(
            passive_library(&bad_header).is_err(),
            "PT_INTERP control before dlopen"
        );
        let functions = cached_elf::functions(&held.bytes, 3).unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions["op_impl_92"].bytes, candidate.image.as_slice());
        assert_eq!(functions["op_impl_92"].address % 64, 0);
        // SAFETY: held DSO is linked only from the mechanically packed, independently decoded image.
        let library =
            unsafe { Library::new(format!("/proc/self/fd/{}", held.file.as_raw_fd())) }.unwrap();
        // SAFETY: exact original header, arity and source rewrite were verified before code loading.
        let writer = *unsafe { library.get::<WriterFn>(b"op_impl_92\0") }.unwrap();
        assert_eq!(writer as usize % 64, 0);
        // SAFETY: the symbol's whole image range is a validated RX section retained by library.
        assert_eq!(
            unsafe { std::slice::from_raw_parts(writer as *const u8, candidate.image.len()) },
            candidate.image.as_slice()
        );
        held.verify().unwrap();
        // SAFETY: independent original91 admission/current bindings are unchanged; default still rejects.
        assert!(unsafe { CpuModel::load(&context.bundle) }.is_err());
        // SAFETY: C's complete immutable SOURCE/object/image/ISA/issuer contract is revalidated on load.
        let mut model =
            unsafe { CpuModel::load_cached(&context.bundle, &context.admission) }.unwrap();
        assert_eq!(model.graph.calls.len(), context.request.calls.len());
        for (actual, expected) in model.graph.calls.iter().zip(&context.request.calls) {
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
        let original_functions: Vec<_> = model
            .calls
            .iter()
            .map(|call| call.function as usize)
            .collect();
        let selected = &model.calls[context.call];
        assert_eq!(
            (
                selected.buffers.len(),
                selected.scalars.len(),
                selected.workers,
                selected.core_id
            ),
            (10, 1, 2, Some(0))
        );
        let original_writer = selected.function;
        WRITER.with(|cell| {
            assert!(cell.replace(Some(writer)).is_none());
        });
        let dispatch_guard = Dispatch;
        model.calls[context.call].function = dispatch;
        for (index, actual) in model.calls.iter().enumerate() {
            if index != context.call {
                assert_eq!(actual.function as usize, original_functions[index]);
            }
        }
        let output = output_root.join(name);
        fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
        let frames: Vec<Frame> = serde_json::from_slice(&context.files["sequence.json"]).unwrap();
        assert_eq!(frames.len(), 4);
        let mut outputs = BTreeMap::new();
        let mut mismatches = Vec::new();
        for (index, frame) in frames.into_iter().enumerate() {
            assert_eq!(
                frame.entrypoint,
                if index % 2 == 0 { "prepare" } else { "policy" }
            );
            assert_eq!(frame.inputs.len(), if index % 2 == 0 { 5 } else { 0 });
            assert_eq!(frame.outputs.len(), if index % 2 == 0 { 0 } else { 6 });
            for (binding, path) in frame.inputs {
                assert_eq!(path.components().count(), 1);
                assert_eq!(
                    path.to_str().unwrap(),
                    format!("{binding}-{}.bin", index / 2)
                );
                model
                    .write_input(&binding, &context.files[path.to_str().unwrap()])
                    .unwrap();
            }
            model.run_entry(&frame.entrypoint).unwrap();
            for (binding, path) in frame.outputs {
                assert_eq!(path.components().count(), 1);
                assert_eq!(
                    path.to_str().unwrap(),
                    format!("{binding}-{}.bin", index / 2)
                );
                let expected = &context.files[&format!("original/{}", path.display())];
                output_total += expected.len();
                assert!(output_total <= 128 * MIB);
                let mut actual = vec![0; expected.len()];
                model.read_output(&binding, &mut actual).unwrap();
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(output.join(&path))
                    .unwrap();
                file.write_all(&actual).unwrap();
                if &actual != expected {
                    mismatches.push(path.clone());
                }
                outputs.insert(
                    path.to_str().unwrap().to_owned(),
                    json!({"bytes":actual.len(),"actual_sha256":sha(&actual),
                    "existing_original_sha256":sha(expected),"equal":&actual==expected}),
                );
            }
        }
        assert_eq!(outputs.len(), 12);
        model.calls[context.call].function = original_writer;
        for (index, actual) in model.calls.iter().enumerate() {
            assert_eq!(actual.function as usize, original_functions[index]);
        }
        drop(dispatch_guard);
        drop(model);
        context.verify();
        variants.insert(
            name,
            json!({"outputs":outputs,"mismatches":mismatches,"all12_equal":mismatches.is_empty(),
            "writer_image_sha256":sha(&candidate.image),"writer_text_bytes":candidate.text_bytes,
            "writer_address":writer as usize,"instructions":candidate.isa.instructions,"required":candidate.isa.required}),
        );
        drop(library);
    }
    context.verify();
    receipt(
        &context.job,
        "writer-pair.json",
        json!({"source":context.request.source_sha256,"diagnostic_complete":true,
        "variants":variants,"compiler":context.compiler,"frames":2,"complete_original_roster":91,"retained_original_programs":90,
        "replaced_calls":[context.call],"new_original_executions":0,"new_taps":0,"b_failure_can_be_promoted":false,
        "original_model_executions":context.compiler["original_model_executions"],
        "original_compiler_invocations":context.compiler["original_compiler_invocations"],
        "default_cached_disabled":true,"candidate_original_certificate_claim":false,"production_admission_changed":false,
        "numerical_acceptance":false,"full128_acceptance":false,"full_model_runtime_acceptance":false,"retirement_credit":0}),
    );
}
