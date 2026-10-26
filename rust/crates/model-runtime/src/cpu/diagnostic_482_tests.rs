//! #482 전용 Actions 관측. production API/execute/수학/기준을 바꾸지 않는다.
#[path = "diagnostic_482_cached.rs"]
mod cached_context;
#[path = "diagnostic_482_pr1344.rs"]
mod pr1344;
#[path = "diagnostic_482_writer.rs"]
mod writer_pair;
use super::*;
use crate::{cached_code, cached_isa};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

const MIB: usize = 1024 * 1024;
const CELLS: [usize; 3] = [100, 101, 1349];
const STORE_SOURCE: &str = "e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: PathBuf,
    sha256: String,
    bytes: usize,
}

fn identity(m: &std::fs::Metadata) -> (u64, u64, u64, u32, u32, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        m.uid(),
        m.nlink(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

struct Held {
    input: Input,
    file: File,
    before: std::fs::Metadata,
    bytes: Vec<u8>,
}
impl Held {
    fn open(input: Input, limit: usize, root: &Path) -> Result<Self, String> {
        Self::open_mode(input, limit, root, true)
    }
    fn open_mode(
        input: Input,
        limit: usize,
        root: &Path,
        materialize: bool,
    ) -> Result<Self, String> {
        let path = &input.path;
        if !path.is_absolute()
            || !path.starts_with(root)
            || path.canonicalize().map_err(|e| e.to_string())? != *path
        {
            return Err("input root/alias".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| e.to_string())?;
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !before.is_file()
            || before.uid() != unsafe { libc::geteuid() }
            || before.len() != input.bytes as u64
            || input.bytes > limit
            || before.nlink() != 1
        {
            return Err("input regular/owner/size/link".into());
        }
        let mut bytes = Vec::new();
        if materialize {
            (&file)
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
        }
        if materialize && (bytes.len() != input.bytes || sha(&bytes) != input.sha256) {
            return Err("input SHA/length".into());
        }
        let held = Self {
            input,
            file,
            before,
            bytes,
        };
        held.verify()?;
        Ok(held)
    }
    fn verify(&self) -> Result<(), String> {
        use std::os::unix::fs::FileExt;
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut chunk = [0u8; 65536];
        loop {
            let count = self
                .file
                .read_at(&mut chunk, offset)
                .map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            offset += count as u64;
            if offset > self.input.bytes as u64 {
                return Err("held growth".into());
            }
            hash.update(&chunk[..count]);
        }
        if identity(&self.file.metadata().map_err(|e| e.to_string())?) != identity(&self.before)
            || identity(&fs::symlink_metadata(&self.input.path).map_err(|e| e.to_string())?)
                != identity(&self.before)
            || format!("{:x}", hash.finalize()) != self.input.sha256
        {
            return Err("held identity/bytes changed".into());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    index: usize,
    kernel: usize,
    source: Input,
    image: Input,
    object: Input,
    compile_event: usize,
    original: cached_code::OriginalProgram,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    index: usize,
    program: Option<usize>,
    kernel: usize,
    workers: u16,
    core_id: Option<usize>,
    views: Vec<usize>,
    scalars: Vec<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u32,
    source_root: PathBuf,
    source_sha256: String,
    job: PathBuf,
    model: Input,
    programs: Vec<Program>,
    calls: Vec<Call>,
    allocations: Vec<usize>,
    views: Vec<crate::graph::View>,
    inputs: BTreeMap<String, usize>,
    outputs: BTreeMap<String, usize>,
    sources: BTreeMap<String, String>,
    events: Input,
}

fn environment() -> (PathBuf, PathBuf) {
    for (key, expected) in [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "bin9208/openpilot-rust"),
        ("RUNNER_ENVIRONMENT", "github-hosted"),
        ("RUNNER_OS", "Linux"),
        ("RUNNER_ARCH", "X64"),
        ("GITHUB_EVENT_NAME", "push"),
        ("GITHUB_REF", "refs/heads/codex/feat-523-python-rust"),
        ("GITHUB_JOB", "model-pipelines"),
    ] {
        assert_eq!(std::env::var(key).unwrap(), expected, "{key}");
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    assert_eq!(
        root,
        PathBuf::from(std::env::var_os("DIAG_SOURCE").unwrap())
    );
    let job = PathBuf::from(std::env::var_os("CI_JOB_TEMP").unwrap());
    assert_eq!(job.canonicalize().unwrap(), job);
    assert_eq!(job.parent().unwrap(), Path::new("/tmp"));
    assert!(job
        .file_name()
        .unwrap()
        .as_encoded_bytes()
        .starts_with(b"openpilot-ci."));
    let metadata = fs::symlink_metadata(&job).unwrap();
    assert!(metadata.is_dir() && metadata.mode() & 0o077 == 0);
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(
        fs::read_to_string(job.join("owner")).unwrap().trim(),
        format!(
            "{}:{}:{}",
            std::env::var("GITHUB_RUN_ID").unwrap(),
            std::env::var("GITHUB_RUN_ATTEMPT").unwrap(),
            std::env::var("GITHUB_JOB").unwrap()
        )
    );
    (root, job)
}

fn request(root: &Path, job: &Path) -> (Request, Vec<Held>) {
    let path = PathBuf::from(std::env::var_os("MODEL482_REQUEST").unwrap());
    let held = Held::open(
        Input {
            bytes: fs::metadata(&path).unwrap().len() as usize,
            path,
            sha256: std::env::var("MODEL482_REQUEST_SHA256").unwrap(),
        },
        4 * MIB,
        job,
    )
    .unwrap();
    let request: Request = serde_json::from_slice(&held.bytes).unwrap();
    assert_eq!(request.schema, 1);
    assert_eq!(request.source_root, root);
    assert_eq!(request.job, job);
    assert_eq!(request.source_sha256, std::env::var("GITHUB_SHA").unwrap());
    assert_eq!(request.source_sha256.len(), 40);
    assert!(request.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut guards = vec![held];
    let sources: [(&str, &[u8]); 19] = [
        (
            "rust/crates/model-runtime/src/guarded_add.rs",
            include_bytes!("../guarded_add.rs"),
        ),
        (
            "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs",
            include_bytes!("../bin/offline-llvm-guarded-add.rs"),
        ),
        (
            "rust/crates/model-runtime/src/owned_outputs.rs",
            include_bytes!("../owned_outputs.rs"),
        ),
        (
            "rust/crates/model-runtime/src/lib.rs",
            include_bytes!("../lib.rs"),
        ),
        (
            "rust/crates/model-runtime/Cargo.toml",
            include_bytes!("../../Cargo.toml"),
        ),
        ("rust/Cargo.toml", include_bytes!("../../../../Cargo.toml")),
        ("rust/Cargo.lock", include_bytes!("../../../../Cargo.lock")),
        (
            "rust/rust-toolchain.toml",
            include_bytes!("../../../../rust-toolchain.toml"),
        ),
        (
            "rust/crates/model-runtime/src/cached_code.rs",
            include_bytes!("../cached_code.rs"),
        ),
        (
            "rust/crates/model-runtime/src/bin/offline-cached-code.rs",
            include_bytes!("../bin/offline-cached-code.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cached_elf.rs",
            include_bytes!("../cached_elf.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cached_isa.rs",
            include_bytes!("../cached_isa.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cpu.rs",
            include_bytes!("../cpu.rs"),
        ),
        (
            "rust/crates/model-runtime/src/graph.rs",
            include_bytes!("../graph.rs"),
        ),
        (
            "rust/crates/model-runtime/src/error.rs",
            include_bytes!("../error.rs"),
        ),
        (
            "rust/crates/model-runtime/src/entrypoint.rs",
            include_bytes!("../entrypoint.rs"),
        ),
        (
            "rust/crates/model-runtime/src/assets.rs",
            include_bytes!("../assets.rs"),
        ),
        (
            "rust/crates/model-runtime/src/buffer.rs",
            include_bytes!("../buffer.rs"),
        ),
        (
            "rust/crates/model-runtime/src/cpu/diagnostic_482_tests.rs",
            include_bytes!("diagnostic_482_tests.rs"),
        ),
    ];
    assert_eq!(request.sources.len(), sources.len());
    let expected_roster: BTreeSet<_> = cached_code::GUARDED_ADD_SOURCES
        .into_iter()
        .chain(cached_code::CACHED_SOURCES)
        .chain(["rust/crates/model-runtime/src/cpu/diagnostic_482_tests.rs"])
        .collect();
    assert_eq!(expected_roster, sources.iter().map(|(p, _)| *p).collect());
    for (relative, bytes) in sources {
        assert_eq!(request.sources[relative], sha(bytes));
        guards.push(
            Held::open(
                Input {
                    path: root.join(relative),
                    bytes: bytes.len(),
                    sha256: sha(bytes),
                },
                32 * MIB,
                root,
            )
            .unwrap(),
        );
    }
    (request, guards)
}

fn roster(request: &Request) -> Result<(), String> {
    if request.programs.is_empty()
        || request.programs.len() > 4096
        || request.calls.len() > 1_000_000
    {
        return Err("complete roster bound".into());
    }
    let mut identities = BTreeSet::new();
    for view in &request.views {
        if view.allocation >= request.allocations.len()
            || view
                .offset
                .checked_add(view.bytes)
                .is_none_or(|end| end > request.allocations[view.allocation])
        {
            return Err("view/backing allocation range".into());
        }
    }
    if request.inputs.is_empty()
        || request.outputs.is_empty()
        || request
            .inputs
            .values()
            .chain(request.outputs.values())
            .any(|v| *v >= request.views.len())
    {
        return Err("complete binding views".into());
    }
    for (index, row) in request.programs.iter().enumerate() {
        let o = &row.original;
        if row.index != index
            || !identities.insert((
                o.source_sha256.clone(),
                o.image_sha256.clone(),
                o.global_slots.clone(),
                o.scalars,
                o.workers,
                o.core_id,
            ))
        {
            return Err("duplicate/index PROGRAM identity".into());
        }
    }
    let mut used = BTreeSet::new();
    for (index, call) in request.calls.iter().enumerate() {
        if call.index != index {
            return Err("call index/order".into());
        }
        if call.views.iter().any(|v| *v >= request.views.len()) {
            return Err("call view range".into());
        }
        if let Some(program) = call.program {
            let row = request
                .programs
                .get(program)
                .ok_or("unknown PROGRAM call")?;
            if call.kernel != row.kernel
                || call.workers != row.original.workers
                || call.core_id != row.original.core_id
                || call.views.len() != row.original.pointers
                || call.scalars.len() != row.original.scalars
            {
                return Err("call PROGRAM/launch contract".into());
            }
            used.insert(program);
        } else if call.workers != 1
            || call.core_id.is_some()
            || call.views.len() != 2
            || !call.scalars.is_empty()
        {
            return Err("COPY launch contract".into());
        }
    }
    if used != (0..request.programs.len()).collect::<BTreeSet<_>>() {
        return Err("missing PROGRAM call".into());
    }
    Ok(())
}

fn receipt(job: &Path, name: &str, value: Value) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(job.join("proof/model482").join(name))
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
}

// 인증된 전체 text의 관측일 뿐이다. production ISA/host admission을 대체하지 않는다.
fn feature_census(images: &[(&[u8], usize)]) -> Result<Value, String> {
    use iced_x86::{Code, Decoder, DecoderOptions};
    if images.is_empty() || images.len() > 4096 {
        return Err("feature census roster bound".into());
    }
    let total = images
        .iter()
        .try_fold(0usize, |total, (image, _)| total.checked_add(image.len()))
        .ok_or("feature census image total overflow")?;
    if total > 64 * MIB {
        return Err("feature census image total bound".into());
    }
    let mut programs = Vec::new();
    let mut first_seen = BTreeMap::new();
    let mut invalid_ranges = Vec::new();
    let mut instructions = 0usize;
    let mut invalid_total = 0usize;
    let mut text_total = 0usize;
    for (index, (image, text)) in images.iter().enumerate() {
        if image.len() > 16 * MIB || *text == 0 || *text > image.len() {
            return Err("feature census executable extent bound".into());
        }
        text_total += *text;
        let mut decoder = Decoder::with_ip(64, &image[..*text], 0, DecoderOptions::NONE);
        let mut features = BTreeSet::new();
        let mut count = 0usize;
        let mut invalid = 0usize;
        let mut privileged = 0usize;
        while decoder.can_decode() {
            if instructions >= 1_000_000 {
                return Err("feature census total instruction count bound".into());
            }
            let start = decoder.position();
            let instruction = decoder.decode();
            let end = decoder.position();
            if end <= start || end > *text || end - start > 15 {
                return Err("feature census decoder progress bound".into());
            }
            instructions += 1;
            count += 1;
            let code = instruction.code();
            if code == Code::INVALID {
                invalid += 1;
                invalid_total += 1;
                if invalid_ranges.len() < 64 {
                    invalid_ranges.push(json!({"program":index,"start":start,"end":end,
                        "bytes":&image[start..end]}));
                }
                continue;
            }
            privileged += usize::from(code.is_privileged());
            for feature in code.cpuid_features() {
                let name = format!("{feature:?}");
                first_seen.entry(name.clone()).or_insert_with(|| {
                    json!({"program":index,"offset":start,
                    "code":format!("{code:?}"),"bytes":&image[start..end]})
                });
                if first_seen.len() > 256 {
                    return Err("feature census feature count bound".into());
                }
                features.insert(name);
            }
        }
        if decoder.position() != *text {
            return Err("feature census incomplete executable extent".into());
        }
        programs.push(
            json!({"index":index,"image_bytes":image.len(),"image_sha256":sha(image),
            "text_bytes":text,"instructions":count,"features":features,
            "invalid_instructions":invalid,"privileged_instructions":privileged}),
        );
    }
    let value = json!({"schema":1,"scope":"authenticated_text_feature_census_only",
        "complete":true,"program_count":programs.len(),"text_bytes":text_total,
        "instructions":instructions,"features":first_seen.keys().collect::<Vec<_>>(),
        "first_seen":first_seen,"programs":programs,"invalid_instructions":invalid_total,
        "invalid_ranges_truncated":invalid_total>invalid_ranges.len(),"invalid_ranges":invalid_ranges,
        "all_decoded_valid":invalid_total==0,"is_isa_admission":false,
        "generated_code_executed":false,"numeric_acceptance":false});
    if serde_json::to_vec_pretty(&value)
        .map_err(|e| e.to_string())?
        .len()
        > MIB
    {
        return Err("feature census metadata1MiB bound".into());
    }
    Ok(value)
}

#[test]
#[ignore = "등록 hosted Actions의 current fresh complete roster/held source가 필요; inert decoder만 실행"]
fn fresh_program_roster_triplets_and_host_isa_are_bound_without_execution() {
    let (root, job) = environment();
    let (mut request, mut guards) = request(&root, &job);
    roster(&request).unwrap();
    // 유효한 complete roster에서 각각 하나만 바꾼다. 설정 오류를 음성 성공으로 삼지 않는다.
    request.calls[0].index += 1;
    assert_eq!(roster(&request).unwrap_err(), "call index/order");
    request.calls[0].index -= 1;
    roster(&request).unwrap();
    let last = request.programs.pop().unwrap();
    assert!(roster(&request).is_err(), "missing PROGRAM must reject");
    request.programs.push(last);
    roster(&request).unwrap();
    request.programs[0].index += 1;
    assert_eq!(
        roster(&request).unwrap_err(),
        "duplicate/index PROGRAM identity"
    );
    request.programs[0].index -= 1;
    let model_view = request.outputs["model"];
    let offset = request.views[model_view].offset;
    request.views[model_view].offset = usize::MAX;
    assert_eq!(
        roster(&request).unwrap_err(),
        "view/backing allocation range"
    );
    request.views[model_view].offset = offset;
    roster(&request).unwrap();
    let mut images = Vec::new();
    let events = Held::open(request.events.clone(), 16 * MIB, &job).unwrap();
    let event_data: Value = serde_json::from_slice(&events.bytes).unwrap();
    let mut failure = None;
    for row in &request.programs {
        let result = (|| -> Result<(Vec<u8>, usize), String> {
            let source = Held::open(row.source.clone(), 16 * MIB, &job)?;
            let image = Held::open(row.image.clone(), 16 * MIB, &job)?;
            let object = Held::open(row.object.clone(), 16 * MIB, &job)?;
            let event = &event_data["chains"][row.compile_event];
            if event["source"]["sha256"] != row.source.sha256
                || event["image"]["sha256"] != row.image.sha256
                || event["object"]["sha256"] != row.object.sha256
            {
                return Err("compile/link event binding".into());
            }
            cached_code::verify_original_program(
                &row.original,
                &source.bytes,
                &image.bytes,
                &object.bytes,
            )?;
            let mut changed = image.bytes.clone();
            changed[0] ^= 1;
            assert!(cached_code::verify_original_program(
                &row.original,
                &source.bytes,
                &changed,
                &object.bytes
            )
            .is_err());
            let mut changed_source = source.bytes.clone();
            changed_source[0] ^= 1;
            assert!(cached_code::verify_original_program(
                &row.original,
                &changed_source,
                &image.bytes,
                &object.bytes
            )
            .is_err());
            let mut changed_object = object.bytes.clone();
            changed_object[0] ^= 1;
            assert!(cached_code::verify_original_program(
                &row.original,
                &source.bytes,
                &image.bytes,
                &changed_object
            )
            .is_err());
            let mut wrong_global = row.original.clone();
            if let Some(first) = wrong_global.global_slots.first_mut() {
                *first = usize::MAX;
                assert!(cached_code::verify_original_program(
                    &wrong_global,
                    &source.bytes,
                    &image.bytes,
                    &object.bytes
                )
                .is_err());
            }
            let mut wrong_worker = row.original.clone();
            wrong_worker.workers = 0;
            assert!(cached_code::verify_original_program(
                &wrong_worker,
                &source.bytes,
                &image.bytes,
                &object.bytes
            )
            .is_err());
            let copy = image.bytes.clone();
            guards.extend([source, image, object]);
            Ok((copy, row.original.text_bytes))
        })();
        match result {
            Ok(image) => images.push(image),
            Err(error) => {
                failure = Some(format!("program{}: {error}", row.index));
                break;
            }
        }
    }
    let host = cached_isa::current_host();
    let references: Vec<_> = images
        .iter()
        .map(|(image, text)| (image.as_slice(), *text))
        .collect();
    let isa = if failure.is_none() {
        cached_isa::admit_all(&references, request.programs.len(), &host)
    } else {
        Err("ISA withheld after triplet rejection".into())
    };
    let census = if failure.is_none() {
        feature_census(&references)
    } else {
        Err("feature census withheld after triplet rejection".into())
    };
    let proofs: Vec<_> = isa
        .as_ref()
        .map(|p| {
            p.iter()
                .enumerate()
                .map(|(i, p)| {
                    json!({"index":i,
        "text_bytes":p.text_bytes,"instructions":p.instructions,"required":p.required})
                })
                .collect()
        })
        .unwrap_or_default();
    if let Ok(proofs) = &isa {
        if let Some(feature) = proofs.iter().flat_map(|p| p.required.iter()).next() {
            let mut denied = host.clone();
            denied.remove(feature);
            assert!(cached_isa::admit_all(&references, request.programs.len(), &denied).is_err());
        }
    }
    guards.push(events);
    // PKL는 실행/역직렬화하지 않고 전체 bytes SHA만 확인한다.
    guards.push(Held::open_mode(request.model.clone(), 128 * MIB, &job, false).unwrap());
    for held in &guards {
        held.verify().unwrap();
    }
    receipt(
        &job,
        "triplet-readonly.json",
        json!({"source":request.source_sha256,
        "complete_roster":request.programs.len(),"calls":request.calls.len(),"triplet_error":failure,
        "strict_triplet_all":failure.is_none(),"host_isa_all":isa.is_ok(),"isa_error":isa.as_ref().err(),
        "feature_census":census.as_ref().ok(),"feature_census_error":census.as_ref().err(),
        "feature_census_is_admission":false,
        "host_features":host,"programs":proofs,"generated_code_executed":false,"scope":"this readonly selector only",
        "cached_issuer_invoked":false,"numeric_acceptance":false,"historical_provider_proven":false}),
    );
    assert!(failure.is_none(), "{failure:?}");
    isa.expect("whole current host ISA rejected; see readonly receipt");
    let _ = census.expect("bounded feature census incomplete; see readonly receipt");
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TapRequest {
    schema: u32,
    triplet_request: Input,
    readonly: Input,
    traffic: PathBuf,
    bundle: PathBuf,
    files: BTreeMap<String, Input>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    entrypoint: String,
    inputs: BTreeMap<String, PathBuf>,
    outputs: BTreeMap<String, PathBuf>,
}
#[derive(Clone)]
struct Event {
    frame: usize,
    worker: usize,
    before: [[u8; 4]; 3],
    after: [[u8; 4]; 3],
    pointers: [usize; 10],
}
struct Tap {
    function: KernelFn,
    address: usize,
    frame: usize,
    events: Vec<Event>,
    error: bool,
}
thread_local! { static TAP: RefCell<Option<Tap>> = const { RefCell::new(None) }; }

// 이 callback에서는 panic/unwind·동기화·JSON 할당 출력을 하지 않는다.
unsafe extern "C" fn tapped(buffers: *const *mut c_void, scalars: *const i32) {
    let state = TAP.try_with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return None;
        };
        let tap = state.as_mut()?;
        if buffers.is_null() || scalars.is_null() || tap.events.len() >= 4 {
            tap.error = true;
            return None;
        }
        // SAFETY: CpuModel이 검증한10인자/1scalar 호출에만 이 test adapter를 등록한다.
        let worker = unsafe { *scalars };
        let mut event = Event {
            frame: tap.frame,
            worker: worker as usize,
            before: [[0; 4]; 3],
            after: [[0; 4]; 3],
            pointers: [0; 10],
        };
        for (i, pointer) in event.pointers.iter_mut().enumerate() {
            *pointer = unsafe { *buffers.add(i) } as usize;
        }
        if !(0..2).contains(&worker) || event.pointers[0] != tap.address {
            tap.error = true;
            return None;
        }
        for (slot, index) in CELLS.iter().enumerate() {
            if index / 1288 == worker as usize {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        (tap.address + index * 4) as *const u8,
                        event.before[slot].as_mut_ptr(),
                        4,
                    )
                };
            }
        }
        Some((tap.function, tap.address, event))
    });
    let Ok(Some((function, address, mut event))) = state else {
        return;
    };
    // SAFETY: 동일 실함수/ABI/인자를 정확1회 전달한다. model이 library와 buffer를 유지한다.
    unsafe { function(buffers, scalars) };
    for (slot, index) in CELLS.iter().enumerate() {
        if index / 1288 == event.worker {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (address + index * 4) as *const u8,
                    event.after[slot].as_mut_ptr(),
                    4,
                )
            };
        }
    }
    let _ = TAP.try_with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            if let Some(tap) = state.as_mut() {
                tap.events.push(event);
            }
        }
    });
}

#[test]
#[ignore = "A 성공 뒤 같은 job의 native bundle/2frame plain 원문과 관측 일치가 필요"]
fn bound_road1928_two_frame_producer_capture() {
    let (root, job) = environment();
    let (request, mut guards) = request(&root, &job);
    let path = PathBuf::from(std::env::var_os("MODEL482_TAP_REQUEST").unwrap());
    let tap_input = Held::open(
        Input {
            bytes: fs::metadata(&path).unwrap().len() as usize,
            path,
            sha256: std::env::var("MODEL482_TAP_REQUEST_SHA256").unwrap(),
        },
        4 * MIB,
        &job,
    )
    .unwrap();
    let capture: TapRequest = serde_json::from_slice(&tap_input.bytes).unwrap();
    assert_eq!(capture.schema, 1);
    assert_eq!(capture.triplet_request.sha256, guards[0].input.sha256);
    let readonly = Held::open(capture.readonly, 4 * MIB, &job).unwrap();
    let proof: Value = serde_json::from_slice(&readonly.bytes).unwrap();
    assert_eq!(proof["strict_triplet_all"], true);
    assert_eq!(proof["host_isa_all"], true);
    assert_eq!(proof["source"], request.source_sha256);
    assert_eq!(proof["complete_roster"], request.programs.len());
    guards.push(readonly);
    assert_eq!(capture.traffic, job.join("build/model482/traffic"));
    assert_eq!(capture.bundle, capture.traffic.join("bundle"));
    let mut files = BTreeMap::new();
    for (name, input) in capture.files {
        assert_eq!(input.path, capture.traffic.join(&name));
        let held = Held::open(input, 512 * MIB, &capture.traffic).unwrap();
        files.insert(name, held.bytes.clone());
        guards.push(held);
    }
    let selected: Vec<_> = request
        .programs
        .iter()
        .filter(|p| p.original.source_sha256 == STORE_SOURCE)
        .collect();
    assert_eq!(
        selected.len(),
        1,
        "known exact source/store context differs; no forcing"
    );
    let selected = selected[0];
    // SAFETY: A의 whole triplet/ISA 통과 뒤 같은 job이 새로 생성하고 hold한 일반 bundle만 선택한다.
    let mut model = unsafe { CpuModel::load(&capture.bundle) }.unwrap();
    assert!(model.graph.cached_code.is_none());
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
    for (native, original) in model.graph.calls.iter().zip(&request.calls) {
        assert_eq!(
            (native.kernel, native.workers, native.core_id),
            (original.kernel, original.workers, original.core_id)
        );
        assert_eq!(native.views, original.views);
        assert_eq!(native.scalars, original.scalars);
    }
    assert_eq!(model.graph.views.len(), request.views.len());
    for (actual, expected) in model.graph.views.iter().zip(&request.views) {
        assert_eq!(
            (actual.allocation, actual.offset, actual.bytes),
            (expected.allocation, expected.offset, expected.bytes)
        );
    }
    assert_eq!(
        model
            .graph
            .allocations
            .iter()
            .map(|v| v.bytes)
            .collect::<Vec<_>>(),
        request.allocations
    );
    let indices: Vec<_> = request
        .calls
        .iter()
        .filter(|c| c.program == Some(selected.index))
        .map(|c| c.index)
        .collect();
    assert_eq!(
        indices.len(),
        1,
        "selected writer must have one call per policy"
    );
    let call_index = indices[0];
    let model_binding = model
        .graph
        .outputs
        .iter()
        .find(|b| b.name == "model")
        .unwrap();
    let output = &model.graph.views[model_binding.view];
    assert_eq!((output.offset, output.bytes), (0, 2576 * 4));
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
    // 다른 call에 같은 allocation을 넘기면 first-writer를 단정하지 않고 관측을 거부한다.
    for (i, other) in model.graph.calls.iter().enumerate() {
        if i != call_index {
            assert!(
                !other
                    .views
                    .iter()
                    .any(|v| model.graph.views[*v].allocation == output.allocation),
                "other alias/producer remains unresolved"
            );
        }
    }
    let function = model.calls[call_index].function;
    TAP.with(|cell| {
        *cell.borrow_mut() = Some(Tap {
            function,
            address,
            frame: 0,
            events: Vec::with_capacity(4),
            error: false,
        })
    });
    // 필수 인자 부재는 실제 kernel forward 전에 거부해야 한다. 이 통제는 수치 실행이 아니다.
    unsafe { tapped(std::ptr::null(), std::ptr::null()) };
    TAP.with(|cell| {
        let mut state = cell.borrow_mut();
        let state = state.as_mut().unwrap();
        assert!(state.error && state.events.is_empty());
        state.error = false;
    });
    model.calls[call_index].function = tapped;
    let frames: Vec<Frame> = serde_json::from_slice(&files["sequence.json"]).unwrap();
    assert_eq!(frames.len(), 4);
    let mut output_hashes = BTreeMap::new();
    for (index, frame) in frames.into_iter().enumerate() {
        assert_eq!(
            frame.entrypoint,
            if index % 2 == 0 { "prepare" } else { "policy" }
        );
        if index % 2 == 0 {
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
        TAP.with(|cell| cell.borrow_mut().as_mut().unwrap().frame = index / 2);
        for (name, path) in frame.inputs {
            assert_eq!(path.components().count(), 1);
            assert_eq!(path.to_str().unwrap(), format!("{name}-{}.bin", index / 2));
            model
                .write_input(&name, &files[path.to_str().unwrap()])
                .unwrap();
        }
        model.run_entry(&frame.entrypoint).unwrap();
        for (name, path) in frame.outputs {
            let filename = path.to_str().unwrap();
            assert_eq!(path.components().count(), 1);
            assert_eq!(filename, format!("{name}-{}.bin", index / 2));
            let mut bytes = vec![0; model.output_size(&name).unwrap()];
            model.read_output(&name, &mut bytes).unwrap();
            assert_eq!(
                bytes, files[filename],
                "native whole-output plain/tapped bytes drift: {filename}"
            );
            output_hashes.insert(filename.to_owned(), sha(&bytes));
        }
    }
    model.calls[call_index].function = function;
    let state = TAP.with(|cell| cell.borrow_mut().take().unwrap());
    assert!(!state.error);
    let mut observed: Vec<_> = state.events.iter().map(|e| (e.frame, e.worker)).collect();
    observed.sort();
    assert_eq!(observed, [(0, 0), (0, 1), (1, 0), (1, 1)]);
    guards.push(tap_input);
    for held in &guards {
        held.verify().unwrap();
    }
    let events: Vec<_> = state.events.iter().map(|e| json!({"frame":e.frame,"worker":e.worker,
        "cells":CELLS.iter().enumerate().filter(|(_,i)| **i/1288==e.worker).map(|(s,i)| json!({"index":i,
            "before_le_bytes":e.before[s],"after_le_bytes":e.after[s]})).collect::<Vec<_>>(),"pointer_args":e.pointers})).collect();
    receipt(
        &job,
        "native-worker-cells.json",
        json!({"events":events,"output_sha256":output_hashes,"plain_tapped_all_outputs_equal":true,
        "first_producer_call_observed":true,"ssa_first_producer_observed":false,"cached_runtime_enabled":false,
        "full128_acceptance":false,"historical_half_snan5_resolved":false}),
    );
}
