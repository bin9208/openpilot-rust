use openpilot_model_runtime::compiler_schedule::{pipeline_entrypoints, PipelineStage};
use serde_json::json;
#[cfg(feature = "native-skip-miri")]
use serde_json::Value;

fn stages(values: &[(&str, &[u64])]) -> Vec<PipelineStage> {
    values
        .iter()
        .map(|(name, calls)| PipelineStage {
            name: (*name).into(),
            calls: calls.to_vec(),
        })
        .collect()
}

#[test]
fn pruned_constants_and_empty_stages_preserve_the_emitted_partition() {
    let stages = stages(&[
        ("prepare", &[10, 11]),
        ("empty", &[]),
        ("policy", &[12, 13, 14]),
        ("removed", &[15]),
    ]);
    let result = pipeline_entrypoints(&stages, &[11, 12, 14], &[0, 1, 2, 3]).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"version":2, "entrypoints":[
            {"name":"prepare","start":0,"end":1},
            {"name":"empty","start":1,"end":1},
            {"name":"policy","start":1,"end":3},
            {"name":"removed","start":3,"end":3}
        ]})
    );
}

#[test]
fn uses_emitted_positions_instead_of_source_or_unique_kernel_counts() {
    // The helper consumes emission accounting rather than guessing a one-to-one
    // mapping: zero/expanded emissions must also retain the stage boundaries.
    let result = pipeline_entrypoints(
        &stages(&[("prepare", &[1]), ("policy", &[2, 3])]),
        &[1, 2, 3],
        &[0, 2, 2, 5],
    )
    .unwrap();
    assert_eq!(result.entrypoints[0].range(), 0..2);
    assert_eq!(result.entrypoints[1].range(), 2..5);
}

#[test]
fn repeated_source_identities_are_counted_at_every_occurrence() {
    let result = pipeline_entrypoints(
        &stages(&[("prepare", &[7, 8]), ("policy", &[7, 9])]),
        &[7, 7, 9],
        &[0, 1, 2, 3],
    )
    .unwrap();
    assert_eq!(result.entrypoints[0].range(), 0..1);
    assert_eq!(result.entrypoints[1].range(), 1..3);
}

#[test]
fn no_stages_selects_v1_and_all_removed_stages_still_select_v2() {
    for (retained, prefix) in [(vec![], vec![0]), (vec![1], vec![0, 1])] {
        let result = pipeline_entrypoints(&[], &retained, &prefix).unwrap();
        assert_eq!(result.version, 1);
        assert!(result.entrypoints.is_empty());
    }
    let result =
        pipeline_entrypoints(&stages(&[("prepare", &[1]), ("policy", &[2])]), &[], &[0]).unwrap();
    assert_eq!(result.version, 2);
    assert!(result
        .entrypoints
        .iter()
        .all(|entry| entry.range() == (0..0)));
}

#[test]
fn rejects_invalid_names_incomplete_coverage_and_stage_overrun() {
    for stages in [
        stages(&[("", &[1])]),
        stages(&[("same", &[]), ("same", &[1])]),
        stages(&[("incomplete", &[])]),
        stages(&[("overrun", &[1, 1])]),
    ] {
        assert!(pipeline_entrypoints(&stages, &[1], &[0, 1]).is_err());
    }
}

#[test]
fn rejects_malformed_emission_prefixes_before_indexing() {
    for prefix in [vec![], vec![1], vec![0, 1], vec![0, 3, 2], vec![0, 1, 2, 3]] {
        assert!(pipeline_entrypoints(&[], &[1, 2], &prefix).is_err());
    }
}

#[test]
fn accepts_exactly_65536_named_empty_stages_and_rejects_one_more() {
    let mut stages = (0..65536)
        .map(|index| PipelineStage {
            name: format!("stage-{index}"),
            calls: vec![],
        })
        .collect::<Vec<_>>();
    let result = pipeline_entrypoints(&stages, &[], &[0]).unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(result.entrypoints.len(), 65536);
    stages.push(PipelineStage {
        name: "one-too-many".into(),
        calls: vec![],
    });
    assert!(pipeline_entrypoints(&stages, &[], &[0]).is_err());
}

// Execute only in Actions. Extract the unchanged boundary statements from the
// actual Python sources; do not substitute candidate-derived expected ranges.
// No tensor, compiler, model, GPU or constant-preparation execution is involved.
#[cfg(feature = "native-skip-miri")]
const ORIGINAL_BOUNDARIES: &str = r#"
import ast, dataclasses, hashlib, json, pathlib, sys, types
root = pathlib.Path(sys.argv[1])
schema_path = root / 'rust/tools/model_export/schema.py'
schedule_path = root / 'rust/tools/model_export/offline_schedule.py'
schema_bytes, schedule_bytes = schema_path.read_bytes(), schedule_path.read_bytes()
schema = ast.parse(schema_bytes, filename=str(schema_path))
schedule = ast.parse(schedule_bytes, filename=str(schedule_path))
scope = {'__name__': '__main__', 'dataclass': dataclasses.dataclass, 'ExportError': RuntimeError}
for name in ('Entrypoint', 'entrypoint_version'):
    node = next(node for node in schema.body if getattr(node, 'name', None) == name)
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(schema_path), 'exec'), scope)
def function(name):
    return next(node for node in schedule.body if getattr(node, 'name', None) == name)
def assigned(node, name):
    return isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == name for target in node.targets)
body = function('export_pipeline').body
first = next(i for i, node in enumerate(body) if assigned(node, 'retained'))
last = next(i for i, node in enumerate(body) if assigned(node, 'model'))
retained_code = compile(ast.Module(body=body[first:last], type_ignores=[]), str(schedule_path), 'exec')
body = function('emit_schedule').body
first = next(i for i, node in enumerate(body) if assigned(node, 'entries'))
last = next(i for i, node in enumerate(body) if assigned(node, 'manifest'))
emitted_code = compile(ast.Module(body=body[first:last], type_ignores=[]), str(schedule_path), 'exec')
manifest = body[last].value
assert isinstance(manifest, ast.Call) and isinstance(manifest.func, ast.Name) and manifest.func.id == 'Manifest'
version_code = compile(ast.Expression(body=manifest.args[0]), str(schedule_path), 'eval')
results = []
for case in json.load(sys.stdin):
    local = dict(scope, pipeline=types.SimpleNamespace(stages=[types.SimpleNamespace(
        name=stage['name'], linear=types.SimpleNamespace(src=stage['calls'])) for stage in case['stages']]),
        prepared=types.SimpleNamespace(linear=types.SimpleNamespace(src=case['retained'])))
    try:
        exec(retained_code, local)
        local.update(schedule=types.SimpleNamespace(stage_ends=local['ends']),
                     emitted_by_source=case['prefix'], calls=range(case['prefix'][-1]))
        exec(emitted_code, local)
        version = eval(version_code, local)
        results.append({'version': version, 'entrypoints': [dataclasses.asdict(entry) for entry in local['entries']]})
    except (RuntimeError, IndexError):
        results.append({'error': True})
print(json.dumps({'source_sha256': {'schema.py': hashlib.sha256(schema_bytes).hexdigest(),
                                  'offline_schedule.py': hashlib.sha256(schedule_bytes).hexdigest()}, 'results': results}))
"#;

#[cfg(feature = "native-skip-miri")]
mod stage_observation {
    use super::{json, Value, ORIGINAL_BOUNDARIES};
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        os::unix::{
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
            process::{CommandExt, ExitStatusExt},
        },
        path::{Path, PathBuf},
        process::{Command, Output, Stdio},
    };

    // stage73 JSON shape 상한: request4,493,165B/stdout3,118,080B.
    // capture 예산이며 기존73 입력/기능 gate의 임계값을 바꾸지 않는다.
    const RECORD_LIMIT: usize = 16 * 1024 * 1024;
    const TOTAL_LIMIT: usize = 64 * 1024 * 1024;
    const RECEIPT_LIMIT: usize = 256 * 1024;

    #[derive(serde::Deserialize)]
    struct OriginalSources {
        source_sha256: BTreeMap<String, String>,
    }

    fn identity(metadata: &fs::Metadata) -> Value {
        json!({"device":metadata.dev(),"inode":metadata.ino(),"bytes":metadata.len(),
            "mode":metadata.mode(),"uid":metadata.uid(),"gid":metadata.gid(),"links":metadata.nlink(),
            "mtime":[metadata.mtime(),metadata.mtime_nsec()],"ctime":[metadata.ctime(),metadata.ctime_nsec()]})
    }

    fn witness(path: &Path) -> std::io::Result<Value> {
        if !path.is_absolute() || path.canonicalize()? != path {
            return Err(std::io::Error::other("canonical absolute witness required"));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let before = file.metadata()?;
        const LIMIT: u64 = 64 * 1024 * 1024;
        if !before.is_file() || before.len() > LIMIT {
            return Err(std::io::Error::other("bounded regular witness required"));
        }
        let mut digest = Sha256::new();
        let mut length = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            length += count as u64;
            if length > LIMIT {
                return Err(std::io::Error::other("witness exceeds bound"));
            }
            digest.update(&buffer[..count]);
        }
        let before = identity(&before);
        if before != identity(&file.metadata()?)
            || before != identity(&fs::symlink_metadata(path)?)
            || before["bytes"] != json!(length)
        {
            return Err(std::io::Error::other("witness changed while hashing"));
        }
        Ok(json!({"path":path,"sha256":format!("{:x}",digest.finalize()),"identity":before}))
    }

    fn record_file(directory: &Path, name: &str) -> File {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(directory.join(name))
            .unwrap()
    }

    fn total(directory: &Path) -> usize {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let metadata = fs::symlink_metadata(entry.unwrap().path()).unwrap();
                assert!(metadata.is_file() && metadata.len() <= RECORD_LIMIT as u64);
                metadata.len() as usize
            })
            .sum()
    }

    fn record(directory: &Path, name: &str, bytes: &[u8]) {
        assert!(bytes.len() <= RECORD_LIMIT && total(directory) + bytes.len() <= TOTAL_LIMIT);
        let mut file = record_file(directory, name);
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn read_bounded(path: &Path, limit: usize) -> Vec<u8> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .unwrap();
        assert!(file.metadata().unwrap().is_file());
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut bytes).unwrap();
        assert!(bytes.len() <= limit);
        bytes
    }

    pub fn capture(root: &Path, request: &[u8]) -> Output {
        let source_root = root.canonicalize().unwrap();
        let admitted = PathBuf::from(std::env::var_os("DIAG_SOURCE").unwrap());
        assert!(admitted.is_absolute());
        assert_eq!(source_root, admitted.canonicalize().unwrap());
        let job = PathBuf::from(std::env::var_os("CI_JOB_TEMP").unwrap());
        let proof = job.join("proof");
        // SAFETY: geteuid는 메모리 인자 없는 현재 uid 조회다.
        let uid = unsafe { libc::geteuid() };
        for directory in [&job, &proof] {
            assert!(directory.is_absolute());
            assert_eq!(directory.canonicalize().unwrap(), *directory);
            let metadata = fs::symlink_metadata(directory).unwrap();
            assert!(metadata.is_dir() && metadata.uid() == uid);
        }
        let owner = format!(
            "{}:{}:{}",
            std::env::var("GITHUB_RUN_ID").unwrap(),
            std::env::var("GITHUB_RUN_ATTEMPT").unwrap(),
            std::env::var("GITHUB_JOB").unwrap()
        );
        assert_eq!(read_bounded(&job.join("owner"), 4096), owner.as_bytes());
        let source = std::env::var("GITHUB_SHA").unwrap();
        assert!(
            source.len() == 40
                && source
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(
            read_bounded(&proof.join("SOURCE_COMMIT"), 4096),
            format!("{source}\n").as_bytes()
        );
        let python = PathBuf::from(
            std::env::var_os("STAGE_REFERENCE_PYTHON")
                .expect("admitted canonical STAGE_REFERENCE_PYTHON required; no PATH fallback"),
        );
        // snippet은 stdlib AST만 사용한다. 선택 원본/native 소스와 외부 admission을 묶으며
        // 전체 import/stdlib/DSO closure를 새로 관측했다고 주장하지 않는다.
        let mut files = [
            ("schema.py", "rust/tools/model_export/schema.py"),
            (
                "offline_schedule.py",
                "rust/tools/model_export/offline_schedule.py",
            ),
            (
                "native-schedule",
                "rust/crates/model-runtime/src/compiler_schedule.rs",
            ),
            (
                "native-entrypoint",
                "rust/crates/model-runtime/src/entrypoint.rs",
            ),
            ("native-lib", "rust/crates/model-runtime/src/lib.rs"),
            (
                "native-test",
                "rust/crates/model-runtime/tests/compiler_schedule.rs",
            ),
            ("native-manifest", "rust/crates/model-runtime/Cargo.toml"),
            ("native-lock", "rust/Cargo.lock"),
        ]
        .into_iter()
        .map(|(name, path)| (name, source_root.join(path)))
        .collect::<BTreeMap<_, _>>();
        files.insert("admitted-python", python.clone());
        files.insert("job-owner", job.join("owner"));
        for name in [
            "SOURCE_COMMIT",
            "stage-oracle-python.sha256",
            "stage-oracle-python.version",
        ] {
            files.insert(name, proof.join(name));
        }
        let before = files
            .iter()
            .map(|(name, path)| (*name, witness(path).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let python_hash = read_bounded(&proof.join("stage-oracle-python.sha256"), 4096);
        let python_version = read_bounded(&proof.join("stage-oracle-python.version"), 4096);
        assert_eq!(
            python_hash,
            format!(
                "{}  {}\n",
                before["admitted-python"]["sha256"].as_str().unwrap(),
                python.to_str().unwrap()
            )
            .as_bytes()
        );
        let version = std::str::from_utf8(&python_version)
            .unwrap()
            .strip_suffix('\n')
            .unwrap();
        assert!(version.starts_with("Python ") && !version.contains(['\n', '\r']));
        let directory = proof.join("compiler-stage-original");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let request_sha = format!("{:x}", Sha256::digest(request));
        for (name, bytes) in [
            ("request.json", request),
            ("oracle.py", ORIGINAL_BOUNDARIES.as_bytes()),
            ("python.sha256", python_hash.as_slice()),
            ("python.version", python_version.as_slice()),
        ] {
            record(&directory, name, bytes);
        }
        assert!(total(&directory) + 2 * RECORD_LIMIT + RECEIPT_LIMIT <= TOTAL_LIMIT);
        let mut command = Command::new(&python);
        command
            .args(["-I", "-c", ORIGINAL_BOUNDARIES])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(record_file(&directory, "stdout.bin")))
            .stderr(Stdio::from(record_file(&directory, "stderr.bin")));
        // SAFETY: fork 뒤에는 setrlimit와 실패 시 errno 조회만 수행한다.
        // raw 파일 각각의 증가를 child에서 제한하며 부모 limit은 바꾸지 않는다.
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: RECORD_LIMIT as libc::rlim_t,
                    rlim_max: RECORD_LIMIT as libc::rlim_t,
                };
                let no_core = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_CORE, &no_core) == 0
                    && libc::setrlimit(libc::RLIMIT_FSIZE, &limit) == 0
                {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
        let mut child = command
            .spawn()
            .expect("Actions must provide admitted canonical Python");
        let input_error = child
            .stdin
            .take()
            .unwrap()
            .write_all(request)
            .err()
            .map(|error| error.to_string());
        let status = child.wait().unwrap();
        let stdout = read_bounded(&directory.join("stdout.bin"), RECORD_LIMIT);
        let stderr = read_bounded(&directory.join("stderr.bin"), RECORD_LIMIT);
        let after = files
            .iter()
            .map(|(name, path)| {
                (
                    *name,
                    witness(path).unwrap_or_else(|error| json!({"error":error.to_string()})),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let unchanged = before == after;
        let request_unchanged = format!(
            "{:x}",
            Sha256::digest(read_bounded(&directory.join("request.json"), RECORD_LIMIT))
        ) == request_sha;
        // results는 기존 비교에서만 Value로 읽는다. 관측 단계에서는 source SHA만
        // 역직렬화하여65,536 entrypoint의 두 번째 임시 Value tree를 만들지 않는다.
        let source_binding = serde_json::from_slice::<OriginalSources>(&stdout)
            .ok()
            .is_some_and(|original| {
                original.source_sha256.len() == 2
                    && ["schema.py", "offline_schedule.py"].iter().all(|name| {
                        original.source_sha256.get(*name).map(String::as_str)
                            == before[*name]["sha256"].as_str()
                    })
            });
        let receipt = json!({"schema_version":1,"kind":"independent-python-stage-observation",
            "collector_only":true,"retirement_credit":0,"github_sha":source,"source_root":source_root,
            "argv":[python.to_str().unwrap(),"-I","-c",ORIGINAL_BOUNDARIES,root.to_str().unwrap()],
            "case_count":73,"request":{"record":"request.json","bytes":request.len(),"sha256":request_sha,"record_unchanged":request_unchanged},
            "original":{"returncode":status.code().or_else(|| status.signal().map(|signal| -signal)),"exit_code":status.code(),"signal":status.signal(),
                "file_size_limit_signal":status.signal() == Some(libc::SIGXFSZ),"input_error":input_error,
                "stdout":{"record":"stdout.bin","bytes":stdout.len(),"sha256":format!("{:x}",Sha256::digest(&stdout))},
                "stderr":{"record":"stderr.bin","bytes":stderr.len(),"sha256":format!("{:x}",Sha256::digest(&stderr))}},
            "record_limit_bytes":RECORD_LIMIT,"stream_limit_bytes":RECORD_LIMIT,
            "stdout_limit_reached":stdout.len() == RECORD_LIMIT,"stderr_limit_reached":stderr.len() == RECORD_LIMIT,
            "total_limit_bytes":TOTAL_LIMIT,"binding_unchanged":unchanged,"original_source_sha_bound":source_binding,
            "witnesses_before":before,"witnesses_after":after,
            "scope":"같은 원본 프로세스 1회의 stage73 raw 관측과 선택 source/Python 전후 binding. 기존 native 비교는 별도로 수행하며 전체 import closure/모델 수용이 아니다."});
        let mut raw = serde_json::to_vec_pretty(&receipt).unwrap();
        raw.push(b'\n');
        assert!(raw.len() <= RECEIPT_LIMIT);
        record(&directory, "observation.json", &raw);
        assert!(
            unchanged,
            "source/Python/proof changed; raw observation retained"
        );
        assert!(
            request_unchanged,
            "request record changed; raw observation retained"
        );
        assert!(
            input_error.is_none(),
            "request write failed; raw observation retained"
        );
        if status.success() {
            assert!(
                source_binding,
                "original source SHA differs; raw observation retained"
            );
        }
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

#[cfg(feature = "native-skip-miri")]
#[test]
#[ignore = "GHA-only original Python boundary comparison; no model/compiled execution"]
fn original_python_pipeline_boundaries_match_native() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let mut cases = Vec::new();
    // Exhaust every removal subset of four source identities across three
    // stages, with a repeated identity straddling the stage boundary.
    for mask in 0..16u64 {
        for repeated in [false, true] {
            let stage_calls: Vec<Vec<u64>> = if repeated {
                vec![vec![0, 1], vec![0, 2], vec![3]]
            } else {
                vec![vec![0, 1], vec![], vec![2, 3]]
            };
            let retained = stage_calls
                .iter()
                .flatten()
                .copied()
                .filter(|id| mask & (1u64 << *id) != 0)
                .collect::<Vec<_>>();
            for expanded in [false, true] {
                let mut prefix = vec![0usize];
                for index in 0..retained.len() {
                    prefix.push(prefix.last().unwrap() + if expanded { index % 3 } else { 1 });
                }
                cases.push(
                    json!({"stages":stage_calls.iter().enumerate().map(|(index, calls)|
                    json!({"name":format!("stage-{index}"),"calls":calls})).collect::<Vec<_>>(),
                    "retained":retained,"prefix":prefix}),
                );
            }
        }
    }
    cases.extend([
        json!({"stages":[],"retained":[],"prefix":[0]}),
        json!({"stages":[],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"","calls":[0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"x","calls":[]},{"name":"x","calls":[0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"missing","calls":[]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"overrun","calls":[0,0]}],"retained":[0],"prefix":[0,1]}),
        json!({"stages":[{"name":"한글 stage","calls":[0]}],"retained":[0],"prefix":[0,1]}),
    ]);
    for count in [65536, 65537] {
        cases.push(json!({"stages":(0..count).map(|index| json!({"name":format!("stage-{index}"),"calls":[]})).collect::<Vec<_>>(),
            "retained":[],"prefix":[0]}));
    }
    assert_eq!(cases.len(), 73);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = stage_observation::capture(&root, &serde_json::to_vec(&cases).unwrap());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let original: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("original boundary sources: {}", original["source_sha256"]);
    assert_eq!(original["results"].as_array().unwrap().len(), cases.len());
    for (index, (case, expected)) in cases
        .iter()
        .zip(original["results"].as_array().unwrap())
        .enumerate()
    {
        let stages: Vec<PipelineStage> = serde_json::from_value(case["stages"].clone()).unwrap();
        let retained: Vec<u64> = serde_json::from_value(case["retained"].clone()).unwrap();
        let prefix: Vec<usize> = serde_json::from_value(case["prefix"].clone()).unwrap();
        let actual = pipeline_entrypoints(&stages, &retained, &prefix);
        if expected.get("error").is_some() {
            assert!(actual.is_err(), "original rejected case {index}");
        } else {
            assert_eq!(
                serde_json::to_value(actual.unwrap()).unwrap(),
                *expected,
                "case {index}"
            );
        }
    }
}
