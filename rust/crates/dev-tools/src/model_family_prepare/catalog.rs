//! Native orchestration of the explicitly pending original compiler entrypoint.
use super::{identity, json_file, real_parents, relative, require, write_json, Input};
use crate::{
    driver_daemon_check::CurrentCatalogAdmission,
    qa_process::{CaptureError, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn super_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn cached_canonical_sha(value: &Value) -> Result<String> {
    // Cached binding fields use the same recursive key ordering as Python's
    // _canonical; serde_json/preserve_order can be enabled by another feature.
    // Sort a clone only. Do not rewrite receipt values or legacy receipt hashes.
    let mut canonical = value.clone();
    canonical.sort_all_objects();
    Ok(super_hash(&serde_json::to_vec(&canonical)?))
}

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct CatalogOptions {
    pub root: PathBuf,
    pub models: PathBuf,
    pub expected_model_manifest: PathBuf,
    pub expected_model_manifest_sha256: String,
    pub kind: CatalogKind,
    pub python: PathBuf,
    pub llvm: PathBuf,
    pub output: PathBuf,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CatalogKind {
    Family,
    Driver,
}
const SOURCES: [&str; 6] = [
    "tinygrad_repo",
    "openpilot/selfdrive/modeld",
    "openpilot/common",
    "openpilot/system/camerad",
    "rust/tools/model_export",
    "uv.lock",
];

struct Snapshot {
    path: PathBuf,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    seal: Value,
}
impl Snapshot {
    fn open(path: &Path, max: u64) -> Result<Self> {
        let input = Input::open(path, max)?;
        Ok(Self {
            path: path.to_owned(),
            identity: input.identity,
            seal: input.seal,
        })
    }
    fn verify(&self, max: u64) -> Result<()> {
        let input = Input::open(&self.path, max)?;
        require(
            input.identity == self.identity && input.seal == self.seal,
            "compiler input changed",
        )
    }
}
fn tree(root: &Path, max_files: usize, max_total: u64) -> Result<BTreeMap<String, Snapshot>> {
    fn visit(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Snapshot>,
        total: &mut u64,
        max_files: usize,
        max_total: u64,
    ) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        require(!m.file_type().is_symlink(), "compiler tree contains alias")?;
        if m.is_dir() {
            if path.file_name().is_some_and(|v| v == "__pycache__") {
                return Ok(());
            }
            for entry in fs::read_dir(path)? {
                visit(root, &entry?.path(), out, total, max_files, max_total)?
            }
        } else {
            require(m.is_file(), "compiler tree contains special file")?;
            *total = total
                .checked_add(m.len())
                .ok_or("compiler tree size overflow")?;
            require(
                out.len() < max_files && *total <= max_total,
                "compiler tree exceeds bound",
            )?;
            let name = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("compiler path is not UTF-8")?
                .to_owned();
            relative(&name)?;
            out.insert(name, Snapshot::open(path, 512 * 1024 * 1024)?);
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    let mut total = 0;
    visit(root, root, &mut out, &mut total, max_files, max_total)?;
    Ok(out)
}
fn seals(map: &BTreeMap<String, Snapshot>) -> Value {
    json!(map
        .iter()
        .map(|(n, p)| (n, p.seal.clone()))
        .collect::<BTreeMap<_, _>>())
}
const HELPER_SOURCES: [&str; 8] = [
    "rust/crates/model-runtime/src/guarded_add.rs",
    "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs",
    "rust/crates/model-runtime/src/owned_outputs.rs",
    "rust/crates/model-runtime/src/lib.rs",
    "rust/crates/model-runtime/Cargo.toml",
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/rust-toolchain.toml",
];
const CACHED_SOURCES: [&str; 9] = [
    "rust/crates/model-runtime/src/cached_code.rs",
    "rust/crates/model-runtime/src/bin/offline-cached-code.rs",
    "rust/crates/model-runtime/src/cached_elf.rs",
    "rust/crates/model-runtime/src/cpu.rs",
    "rust/crates/model-runtime/src/graph.rs",
    "rust/crates/model-runtime/src/error.rs",
    "rust/crates/model-runtime/src/entrypoint.rs",
    "rust/crates/model-runtime/src/assets.rs",
    "rust/crates/model-runtime/src/buffer.rs",
];
const PUBLISHER_SOURCES: [&str; 5] = [
    "rust/crates/model-runtime/src",
    "rust/crates/model-runtime/Cargo.toml",
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/rust-toolchain.toml",
];
fn source_tree(
    root: &Path,
    guarded_add: bool,
    cached: bool,
    publisher: bool,
) -> Result<BTreeMap<String, Snapshot>> {
    let mut out = BTreeMap::new();
    for name in SOURCES {
        let base = root.join(name);
        let metadata = fs::symlink_metadata(&base)?;
        if metadata.is_dir() {
            for (relative, pin) in tree(&base, 5000, 1024 * 1024 * 1024)? {
                out.insert(format!("{name}/{relative}"), pin);
            }
        } else {
            out.insert(name.to_owned(), Snapshot::open(&base, 512 * 1024 * 1024)?);
        }
        require(out.len() <= 5000, "compiler source roster exceeds bound")?;
    }
    if guarded_add {
        for name in HELPER_SOURCES {
            out.insert(
                name.to_owned(),
                Snapshot::open(&root.join(name), 16 * 1024 * 1024)?,
            );
            require(out.len() <= 5000, "compiler source roster exceeds bound")?;
        }
    }
    if cached {
        for name in CACHED_SOURCES {
            out.insert(
                name.to_owned(),
                Snapshot::open(&root.join(name), 16 * 1024 * 1024)?,
            );
        }
    }
    if publisher {
        // Hold the complete linked native library source, including the disk
        // guard and owned-output implementation, rather than only its CLI.
        for name in PUBLISHER_SOURCES {
            let base = root.join(name);
            if fs::symlink_metadata(&base)?.is_dir() {
                for (relative, pin) in tree(&base, 5000, 1024 * 1024 * 1024)? {
                    out.insert(format!("{name}/{relative}"), pin);
                }
            } else {
                out.insert(name.to_owned(), Snapshot::open(&base, 16 * 1024 * 1024)?);
            }
            require(out.len() <= 5000, "compiler source roster exceeds bound")?;
        }
    }
    Ok(out)
}
fn guarded_helper() -> Result<Option<Input>> {
    configured_helper("MODEL_LLVM_GUARDED_ADD")
}
fn configured_helper(prefix: &str) -> Result<Option<Input>> {
    let path = std::env::var_os(format!("{prefix}_BINARY"));
    let hash = std::env::var(format!("{prefix}_SHA256"));
    match (path, hash) {
        (None, Err(std::env::VarError::NotPresent)) => Ok(None),
        (Some(path), Ok(hash)) => {
            require(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid guarded add helper SHA256",
            )?;
            let input = Input::open(Path::new(&path), 128 * 1024 * 1024)?;
            require(
                input.path == fs::canonicalize(&input.path)? && input.seal["sha256"] == hash,
                "guarded add helper path/hash differs",
            )?;
            let m = input.file.metadata()?;
            let mut header = [0_u8; 64];
            require(
                std::os::unix::fs::FileExt::read_at(&input.file, &mut header, 0)? == header.len()
                    && &header[..7] == b"\x7fELF\x02\x01\x01"
                    && matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
                    && u16::from_le_bytes([header[18], header[19]])
                        == if cfg!(target_arch = "aarch64") {
                            183
                        } else {
                            62
                        }
                    && m.mode() & 0o111 != 0,
                "guarded add helper is not a native executable ELF",
            )?;
            Ok(Some(input))
        }
        _ => Err("guarded add requires paired explicit ELF path and SHA256".into()),
    }
}
fn helper_binding(input: &Input) -> Result<Value> {
    input.verify()?;
    let m = input.file.metadata()?;
    Ok(
        json!({"enabled":true,"path":input.path,"bytes":m.len(),"sha256":input.seal["sha256"],
        "identity":[m.dev(),m.ino(),m.len(),u64::from(m.mode()),i128::from(m.mtime())*1_000_000_000+i128::from(m.mtime_nsec()),i128::from(m.ctime())*1_000_000_000+i128::from(m.ctime_nsec())]}),
    )
}
fn publisher_target(target: &Value, binding: Option<&Value>) -> Result<()> {
    match binding {
        Some(binding) => require(
            target["catalog_publisher_before"] == *binding
                && target["catalog_publisher_after"] == *binding,
            "catalog publisher compiler receipt differs from held ELF",
        ),
        None => require(
            target["catalog_publisher_before"].is_null()
                && target["catalog_publisher_after"].is_null(),
            "unadmitted catalog publisher compiler receipt",
        ),
    }
}
fn verify_map(map: &BTreeMap<String, Snapshot>) -> Result<()> {
    for pin in map.values() {
        pin.verify(512 * 1024 * 1024)?
    }
    Ok(())
}
fn host(llvm: &Input) -> Result<Value> {
    let text = fs::read_to_string("/proc/cpuinfo")?;
    let mut profiles = Vec::new();
    for section in text.split("\n\n") {
        let fields = section
            .lines()
            .filter_map(|v| v.split_once(':'))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect::<BTreeMap<_, _>>();
        if let (Some(model), Some(flags)) = (fields.get("model name"), fields.get("flags")) {
            let flags = flags
                .split_whitespace()
                .collect::<std::collections::BTreeSet<_>>();
            let profile = json!({"model_name":model,"flags":flags});
            if !profiles.contains(&profile) {
                profiles.push(profile)
            }
        }
    }
    require(!profiles.is_empty(), "current CPU profile missing")?;
    llvm.verify()?;
    Ok(
        json!({"machine":std::env::consts::ARCH,"proc_cpu":profiles,"llvm_path":llvm.path,"llvm_sha256":llvm.seal["sha256"]}),
    )
}
fn execute(spec: &CommandSpec, output: &Path) -> Result<crate::qa_process::Captured> {
    write_json(
        &output.join("compiler-invocation.json"),
        &json!({"argv":spec.effective_argv().iter().map(|v|v.to_string_lossy()).collect::<Vec<_>>(),"environment":spec.env,"deadline_seconds":spec.timeout.as_secs()}),
    )?;
    let stdout = output.join("compiler.stdout");
    let stderr = output.join("compiler.stderr");
    // Keep the runner's live limits: file-backed capture deliberately bypasses
    // them. Persist exposed partial buffers on failure without claiming that a
    // Limit error (which currently exposes no buffers) contains captured bytes.
    let result = NativeRunner.run(spec);
    match result {
        Ok(result) => {
            fs::write(&stdout, &result.stdout)?;
            fs::write(&stderr, &result.stderr)?;
            write_json(
                &output.join("compiler-status.json"),
                &json!({"code":result.code,"stdout_bytes":result.stdout.len(),"stderr_bytes":result.stderr.len(),"stream_budget_enforcement":"live bounded capture"}),
            )?;
            Ok(result)
        }
        Err(error) => {
            let (out, err, available) = match error.downcast_ref::<CaptureError>() {
                Some(CaptureError::Timeout { stdout, stderr, .. }) => {
                    (stdout.as_slice(), stderr.as_slice(), true)
                }
                _ => (&[][..], &[][..], false),
            };
            fs::write(&stdout, out)?;
            fs::write(&stderr, err)?;
            write_json(
                &output.join("compiler-status.json"),
                &json!({"error":error.to_string(),"partial_streams_available":available,"partial_streams_note":if available {"timeout partial buffers preserved"} else {"existing bounded runner exposes no partial buffers for this error; empty diagnostic files do not establish empty child streams"},"stdout_bytes":out.len(),"stderr_bytes":err.len(),"stream_budget_enforcement":"live bounded capture"}),
            )?;
            Err(error)
        }
    }
}

pub fn prepare_catalog(options: &CatalogOptions) -> Result<Value> {
    real_parents(&options.output)?;
    require(
        fs::symlink_metadata(&options.output)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "catalog preparation output exists or is inaccessible",
    )?;
    for path in [&options.root, &options.models] {
        real_parents(path)?;
        require(
            fs::symlink_metadata(path)?.is_dir(),
            "catalog preparation root is not a directory",
        )?;
    }
    crate::disk_space::inspect(&options.output, 3 * 1024 * 1024 * 1024)?.require()?;
    let manifest = Input::open(&options.expected_model_manifest, 2 * 1024 * 1024)?;
    require(
        manifest.seal["sha256"] == options.expected_model_manifest_sha256,
        "original model manifest pin mismatch",
    )?;
    let original = json_file(&options.expected_model_manifest)?;
    let models_value = if options.kind == CatalogKind::Driver {
        &original["original_models"]
    } else {
        &original["models"]
    };
    let expected = models_value
        .as_object()
        .ok_or("original model roster missing")?;
    let wanted = if options.kind == CatalogKind::Driver {
        4
    } else {
        5
    };
    require(
        expected.len() == wanted,
        "original model roster count mismatch",
    )?;
    let mut names = vec![
        "dmonitoring_model_tinygrad.pkl",
        "dmonitoring_model_metadata.pkl",
        "dm_warp_1344x760_tinygrad.pkl",
        "dm_warp_1928x1208_tinygrad.pkl",
    ];
    if options.kind == CatalogKind::Family {
        names.push("driving_tinygrad.pkl");
    }
    require(
        expected
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            == names.into_iter().collect(),
        "original model filenames differ from compiler contract",
    )?;
    let models = tree(&options.models, 5, 128 * 1024 * 1024)?;
    require(
        seals(&models) == *models_value,
        "actual model files differ from independent original full roster",
    )?;
    let helper = guarded_helper()?;
    let helper_before = helper.as_ref().map(helper_binding).transpose()?;
    let cached_helper = configured_helper("MODEL_CACHED_CODE")?;
    let cached_before = cached_helper.as_ref().map(helper_binding).transpose()?;
    let publisher = configured_helper("MODEL_CATALOG_PUBLISH")
        .map_err(|error| format!("catalog publisher admission failed: {error}"))?;
    let publisher_before = publisher.as_ref().map(helper_binding).transpose()?;
    let sources = source_tree(
        &options.root,
        helper.is_some(),
        cached_helper.is_some(),
        publisher.is_some(),
    )?;
    let cached_request = match (
        std::env::var_os("MODEL_CACHED_CODE_REQUEST"),
        std::env::var("MODEL_CACHED_CODE_REQUEST_SHA256"),
        &cached_before,
    ) {
        (None, Err(std::env::VarError::NotPresent), None) => None,
        (Some(path), Ok(hash), Some(binding)) => {
            let pin = Input::open(Path::new(&path), 4 * 1024 * 1024)?;
            require(
                pin.path == fs::canonicalize(&pin.path)? && pin.seal["sha256"] == hash,
                "cached request path/SHA differs",
            )?;
            let request = json_file(&pin.path)?;
            let current = &request["admission"]["current"];
            require(
                request["source_root"] == options.root.to_str().ok_or("source root UTF8")?
                    && request["admission"]["original_model_sha256"]
                        == models["dmonitoring_model_tinygrad.pkl"].seal["sha256"]
                    && current["cached_helper_sha256"] == binding["sha256"]
                    && current["cached_helper_binding_sha256"] == cached_canonical_sha(binding)?
                    && current["helper_binding_sha256"]
                        == cached_canonical_sha(
                            &helper_before
                                .clone()
                                .ok_or("cached mode requires guarded helper")?,
                        )?
                    && current["exporter_roster_sha256"] == cached_canonical_sha(&seals(&sources))?,
                "cached request authentic original/current source/helper binding differs",
            )?;
            for (field, names) in [
                ("guarded_add_sources", &HELPER_SOURCES[..]),
                ("cached_sources", &CACHED_SOURCES[..]),
            ] {
                let expected = names
                    .iter()
                    .map(|name| json!({"path":name,"sha256":sources[*name].seal["sha256"]}))
                    .collect::<Vec<_>>();
                require(
                    current[field] == json!(expected),
                    "cached issuer exact source roster differs",
                )?;
            }
            Some((pin, request))
        }
        _ => return Err("cached mode requires paired native ELF and request path/SHA".into()),
    };
    let compiler = Input::open(
        &options.root.join("rust/tools/build_model_pipelines.py"),
        1024 * 1024,
    )?;
    let python = Input::open(&fs::canonicalize(&options.python)?, 128 * 1024 * 1024)?;
    let llvm_path = fs::canonicalize(&options.llvm)?;
    require(
        fs::canonicalize(std::env::var_os("LLVM_PATH").ok_or("LLVM_PATH is required")?)?
            == llvm_path,
        "configured LLVM differs from preparation input",
    )?;
    let llvm = Input::open(&llvm_path, 512 * 1024 * 1024)?;
    let before = host(&llvm)?;
    let parent = options
        .output
        .parent()
        .ok_or("catalog output parent missing")?;
    let temp = tempfile::Builder::new()
        .prefix(".model-family-catalog-")
        .tempdir_in(parent)?;
    let catalog = temp.path().join("catalog");
    let mut argv = vec![
        python.path.as_os_str().to_owned(),
        "-P".into(),
        compiler.path.as_os_str().to_owned(),
        "--trusted-model-directory".into(),
        options.models.as_os_str().to_owned(),
        "--backend".into(),
        "cpu".into(),
        "--output".into(),
        catalog.as_os_str().to_owned(),
        "--target-receipt".into(),
        temp.path().join("compiler-target.json").into_os_string(),
    ];
    if options.kind == CatalogKind::Driver {
        argv.extend([OsString::from("--kind"), OsString::from("driver")]);
    }
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(900));
    spec.cwd = Some(options.root.clone());
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec.env = BTreeMap::from([
        (
            "PYTHONPATH".to_owned(),
            format!(
                "{}:{}:{}",
                options.root.display(),
                options.root.join("tinygrad_repo").display(),
                options.root.join("rust/tools").display()
            ),
        ),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
        ("DEV".into(), "CPU:LLVM".into()),
        ("JIT".into(), "1".into()),
        ("JIT_BATCH_SIZE".into(), "0".into()),
        ("OPENPILOT_HACKS".into(), "1".into()),
        ("CPU_COUNT".into(), "2".into()),
        (
            "LLVM_PATH".into(),
            llvm_path
                .to_str()
                .ok_or("LLVM path is not UTF-8")?
                .to_owned(),
        ),
    ]);
    if let Some(input) = &helper {
        spec.env.insert(
            "MODEL_LLVM_GUARDED_ADD_BINARY".into(),
            input
                .path
                .to_str()
                .ok_or("guarded add helper path is not UTF-8")?
                .to_owned(),
        );
        spec.env.insert(
            "MODEL_LLVM_GUARDED_ADD_SHA256".into(),
            input.seal["sha256"]
                .as_str()
                .ok_or("guarded add helper hash missing")?
                .to_owned(),
        );
    }
    if let (Some(input), Some((request, _))) = (&cached_helper, &cached_request) {
        for (name, value) in [
            (
                "MODEL_CACHED_CODE_BINARY",
                input.path.to_str().ok_or("cached helper UTF8")?.to_owned(),
            ),
            (
                "MODEL_CACHED_CODE_SHA256",
                input.seal["sha256"]
                    .as_str()
                    .ok_or("cached helper SHA")?
                    .to_owned(),
            ),
            (
                "MODEL_CACHED_CODE_REQUEST",
                request
                    .path
                    .to_str()
                    .ok_or("cached request UTF8")?
                    .to_owned(),
            ),
            (
                "MODEL_CACHED_CODE_REQUEST_SHA256",
                request.seal["sha256"]
                    .as_str()
                    .ok_or("cached request SHA")?
                    .to_owned(),
            ),
        ] {
            spec.env.insert(name.into(), value);
        }
    }
    if let Some(input) = &publisher {
        spec.env.insert(
            "MODEL_CATALOG_PUBLISH_BINARY".into(),
            input
                .path
                .to_str()
                .ok_or("catalog publisher path UTF8")?
                .to_owned(),
        );
        spec.env.insert(
            "MODEL_CATALOG_PUBLISH_SHA256".into(),
            input.seal["sha256"]
                .as_str()
                .ok_or("catalog publisher SHA missing")?
                .to_owned(),
        );
    }
    let result = execute(&spec, temp.path());
    if !matches!(&result, Ok(result) if result.code == 0) {
        let evidence = temp.keep();
        return Err(format!(
            "pending compiler entrypoint failed; invocation/status/available stream diagnostics at {}",
            evidence.display()
        )
        .into());
    }
    manifest.verify()?;
    compiler.verify()?;
    python.verify()?;
    verify_map(&models)?;
    verify_map(&sources)?;
    require(
        seals(&source_tree(
            &options.root,
            helper.is_some(),
            cached_helper.is_some(),
            publisher.is_some(),
        )?) == seals(&sources)
            && seals(&tree(&options.models, 5, 128 * 1024 * 1024)?) == seals(&models),
        "compiler source/model roster changed",
    )?;
    require(before == host(&llvm)?, "compiler host identity changed")?;
    let target_pin = Input::open(&temp.path().join("compiler-target.json"), 1024 * 1024)?;
    let target = json_file(&target_pin.path)?;
    require(
        target["version"] == 1
            && target["stored_jit_executed"] == false
            && target["before"] == target["after"],
        "compiler target changed or old JIT was executed",
    )?;
    publisher_target(&target, publisher_before.as_ref())?;
    require(
        publisher.as_ref().map(helper_binding).transpose()? == publisher_before,
        "catalog publisher changed during compilation",
    )?;
    let target_helper = &target["guarded_add_before"];
    require(
        target_helper == &target["guarded_add_after"],
        "guarded add compiler binding changed",
    )?;
    match &helper_before {
        Some(binding) => require(
            target_helper == binding,
            "guarded add compiler helper differs from held ELF",
        )?,
        None => require(
            target_helper.is_null() || target_helper == &json!({"enabled":false}),
            "unadmitted guarded add helper",
        )?,
    }
    require(
        helper.as_ref().map(helper_binding).transpose()? == helper_before,
        "guarded add helper changed during compilation",
    )?;
    let cached_target = match (&cached_request, &cached_before) {
        (Some((pin, _)), Some(binding)) => {
            pin.verify()?;
            require(
                cached_helper.as_ref().map(helper_binding).transpose()? == cached_before,
                "cached native issuer changed during compilation",
            )?;
            let before = &target["cached_code_before"];
            require(
                before == &target["cached_code_after"]
                    && before["enabled"] == true
                    && before["helper"] == *binding
                    && before["request_sha256"] == pin.seal["sha256"]
                    && before["complete_preflight"] == true
                    && before["generated_code_executed"] == false,
                "cached compiler native preflight binding differs",
            )?;
            Some(before.clone())
        }
        (None, None) => {
            require(
                target["cached_code_before"].is_null() && target["cached_code_after"].is_null(),
                "unrecorded cached compiler mode",
            )?;
            None
        }
        _ => return Err("cached compiler configuration differs".into()),
    };
    let target_host = &target["before"];
    require(
        target_host["llvm_path"] == before["llvm_path"]
            && target_host["llvm_sha256"] == before["llvm_sha256"]
            && target_host["llvm_cpu"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            && target_host["llvm_features"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string)),
        "compiler actual LLVM target identity missing or differs",
    )?;
    let mut receipt_host = before.clone();
    receipt_host["llvm_cpu"] = target_host["llvm_cpu"].clone();
    receipt_host["llvm_features"] = target_host["llvm_features"].clone();
    target_pin.verify()?;
    let files = tree(&catalog, 2000, 2 * 1024 * 1024 * 1024)?;
    let pointer = json_file(&catalog.join("current.json"))?;
    let generation = pointer["generation"]
        .as_str()
        .ok_or("catalog generation missing")?;
    relative(generation)?;
    let index = json_file(&catalog.join(generation).join("index.json"))?;
    require(
        files
            .get(&format!("{generation}/index.json"))
            .is_some_and(|p| p.seal["sha256"] == generation),
        "catalog generation hash mismatch",
    )?;
    require(
        index["bundles"].as_array().is_some_and(|b| {
            b.len()
                == if options.kind == CatalogKind::Driver {
                    2
                } else {
                    4
                }
        }),
        "catalog bundle count mismatch",
    )?;
    let mut bundles = std::collections::BTreeSet::new();
    for bundle in index["bundles"]
        .as_array()
        .ok_or("catalog bundles missing")?
    {
        let kind = bundle["kind"].as_str().ok_or("catalog kind missing")?;
        let camera = bundle["camera"]
            .as_array()
            .ok_or("catalog camera missing")?;
        require(camera.len() == 2, "catalog camera shape invalid")?;
        let width = camera[0].as_u64().ok_or("catalog camera width missing")?;
        let height = camera[1].as_u64().ok_or("catalog camera height missing")?;
        require(
            matches!((width, height), (1344, 760) | (1928, 1208))
                && (kind == "driver" || (options.kind == CatalogKind::Family && kind == "driving"))
                && bundles.insert((kind.to_owned(), width, height)),
            "catalog bundle kind/camera duplicate or invalid",
        )?;
        let directory = bundle["directory"]
            .as_str()
            .ok_or("bundle directory missing")?;
        relative(directory)?;
        let descriptor = json_file(
            &catalog
                .join(generation)
                .join(directory)
                .join("pipeline.json"),
        )?;
        let expected = if kind == "driver" {
            json!({"model":models["dmonitoring_model_tinygrad.pkl"].seal["sha256"],"metadata":models["dmonitoring_model_metadata.pkl"].seal["sha256"],"warp":models[&format!("dm_warp_{width}x{height}_tinygrad.pkl")].seal["sha256"]})
        } else {
            json!({"driving":models["driving_tinygrad.pkl"].seal["sha256"]})
        };
        if kind == "driver" {
            let graph = json_file(&catalog.join(generation).join(directory).join("graph.json"))?;
            match &cached_request {
                Some((_, request)) => {
                    let admission_path = catalog
                        .join(generation)
                        .join(directory)
                        .join("cached-admission.json");
                    let admission = Input::open(&admission_path, 4 * 1024 * 1024)?;
                    require(
                        graph["backend"] == "cpu-cached-llvm"
                            && graph["version"] == 3
                            && json_file(&admission_path)? == request["admission"]
                            && cached_target.as_ref().is_some_and(|target| {
                                target["admission_sha256"] == admission.seal["sha256"]
                            }),
                        "cached catalog graph or independently expected admission differs",
                    )?;
                }
                None => require(
                    graph["backend"] != "cpu-cached-llvm" && graph["cached_code"].is_null(),
                    "unadmitted cached graph",
                )?,
            }
        }
        require(
            descriptor["sources"] == expected
                && descriptor["kind"] == kind
                && descriptor["camera"] == bundle["camera"],
            "exported pipeline source/model binding differs",
        )?;
    }
    let mut git = CommandSpec::new_os(
        vec![
            "git".into(),
            "-C".into(),
            options.root.as_os_str().to_owned(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        Duration::from_secs(10),
    );
    git.stdout_limit = 256;
    git.stderr_limit = 1024;
    let checkout = NativeRunner.run(&git)?;
    require(checkout.code == 0, "checkout identity unavailable")?;
    let checkout = std::str::from_utf8(&checkout.stdout)?.trim();
    require(
        checkout.len() == 40 && checkout.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid checkout identity",
    )?;
    let mut report = json!({"version":1,"provider":"CPU:LLVM","source_commit":checkout,"host":receipt_host,"models":seals(&models),"exporter_dependency_files":seals(&sources),"catalog_generation":generation,"catalog_files":seals(&files),"stored_jit_executed":false,"original_jit_execution_admitted":false,"compiler_entrypoint":compiler.seal,"compiler_interpreter":python.seal,"compiler_target_receipt":target_pin.seal,"original_model_manifest":manifest.seal,"scope":"native orchestration; existing Python compiler remains pending retirement"});
    if let Some(binding) = &publisher_before {
        report["catalog_publisher"] = binding.clone();
    }
    if let Some(binding) = &helper_before {
        report["guarded_add"] = binding.clone();
    }
    if let (Some(target), Some((request, _))) = (&cached_target, &cached_request) {
        report["cached_code"] = target.clone();
        report["cached_code"]["request"] = json!({"path":request.path,"sha256":request.seal["sha256"],"bytes":request.seal["bytes"]});
    }
    let receipt = temp.path().join("current-catalog-receipt.json");
    write_json(&receipt, &report)?;
    let receipt_pin = Input::open(&receipt, 4_000_000)?;
    let hashes = models
        .iter()
        .map(|(n, p)| {
            Ok((
                n.clone(),
                p.seal["sha256"]
                    .as_str()
                    .ok_or("model SHA missing")?
                    .to_owned(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    CurrentCatalogAdmission::open(
        &options.root,
        &catalog,
        &receipt,
        receipt_pin.seal["sha256"]
            .as_str()
            .ok_or("receipt SHA missing")?,
        &hashes,
        checkout,
    )?
    .verify()?;
    // Rename the complete owned bundle, then re-open admission at its final paths.
    let directory = fs::File::open(temp.path())?;
    let before_directory = directory.metadata()?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temp.path(),
        rustix::fs::CWD,
        &options.output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _ = temp.keep();
    let mut published = super::usb::Published {
        path: options.output.clone(),
        file: directory,
        accepted: false,
    };
    let after = fs::symlink_metadata(&options.output)?;
    require(
        after.is_dir()
            && !after.file_type().is_symlink()
            && (after.dev(), after.ino()) == (before_directory.dev(), before_directory.ino()),
        "catalog output replaced during publication",
    )?;
    CurrentCatalogAdmission::open(
        &options.root,
        &options.output.join("catalog"),
        &options.output.join("current-catalog-receipt.json"),
        receipt_pin.seal["sha256"]
            .as_str()
            .ok_or("receipt SHA missing")?,
        &hashes,
        checkout,
    )?
    .verify()?;
    manifest.verify()?;
    compiler.verify()?;
    python.verify()?;
    verify_map(&models)?;
    verify_map(&sources)?;
    require(before == host(&llvm)?, "final compiler host changed")?;
    if let Some((request, _)) = &cached_request {
        request.verify()?;
    }
    require(
        cached_helper.as_ref().map(helper_binding).transpose()? == cached_before,
        "final cached native issuer changed",
    )?;
    require(
        Input::open(&options.output.join("compiler-target.json"), 1024 * 1024)?.seal
            == target_pin.seal,
        "published compiler target receipt changed",
    )?;
    require(
        seals(&tree(&options.models, 5, 128 * 1024 * 1024)?) == seals(&models),
        "final model roster changed",
    )?;
    require(
        identity(&published.file.metadata()?) == identity(&fs::symlink_metadata(&options.output)?),
        "catalog publication replaced",
    )?;
    require(
        helper.as_ref().map(helper_binding).transpose()? == helper_before,
        "published guarded add helper changed",
    )?;
    require(
        publisher.as_ref().map(helper_binding).transpose()? == publisher_before,
        "published catalog publisher changed",
    )?;
    published.accepted = true;
    Ok(report)
}

pub fn catalog_cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--root", true),
            OptionSpec::value("--models", true),
            OptionSpec::value("--expected-model-manifest", true),
            OptionSpec::value("--expected-model-manifest-sha256", true),
            OptionSpec::value("--kind", true),
            OptionSpec::value("--python", true),
            OptionSpec::value("--llvm", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!(
            "Usage: prepare-model-family-catalog --root ROOT --models DIR --expected-model-manifest JSON --expected-model-manifest-sha256 SHA256 --kind family|driver --python PYTHON --llvm LIB --output DIR"
        );
        return Ok(());
    }
    let value = |name: &str| -> Result<&std::ffi::OsStr> {
        Ok(parsed
            .values
            .get(name)
            .ok_or("catalog preparation option missing")?
            .as_os_str())
    };
    let kind = match value("--kind")?.to_str() {
        Some("family") => CatalogKind::Family,
        Some("driver") => CatalogKind::Driver,
        _ => return Err("catalog kind must be family or driver".into()),
    };
    let report = prepare_catalog(&CatalogOptions {
        root: value("--root")?.into(),
        models: value("--models")?.into(),
        expected_model_manifest: value("--expected-model-manifest")?.into(),
        expected_model_manifest_sha256: value("--expected-model-manifest-sha256")?
            .to_str()
            .ok_or("manifest SHA is not UTF-8")?
            .to_owned(),
        kind,
        python: value("--python")?.into(),
        llvm: value("--llvm")?.into(),
        output: value("--output")?.into(),
    })?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn compiler_failure_and_timeout_retain_logs_while_live_limit_records_unavailable_partials() {
        for (body, timeout, limit, expected_error, expected_code) in [
            (
                "printf 'out\\377'; printf 'err\\r' >&2; exit 7",
                Duration::from_secs(5),
                64,
                false,
                Some(7),
            ),
            (
                "printf 'out\\377'; printf 'err\\r' >&2; exec sleep 30",
                Duration::from_secs(2),
                64,
                true,
                None,
            ),
            (
                "printf 'out\\377'; printf 'err\\r' >&2; exec sleep 30",
                Duration::from_secs(5),
                3,
                true,
                None,
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let mut spec =
                CommandSpec::new(vec!["/bin/sh".into(), "-c".into(), body.into()], timeout);
            spec.stdout_limit = limit;
            spec.stderr_limit = limit;
            let result = execute(&spec, temp.path());
            assert_eq!(result.is_err(), expected_error);
            let status = json_file(&temp.path().join("compiler-status.json")).unwrap();
            if limit == 3 {
                assert!(matches!(
                    result.unwrap_err().downcast_ref::<CaptureError>(),
                    Some(CaptureError::Limit { limit: 3, .. })
                ));
                assert_eq!(status["partial_streams_available"], false);
                assert!(fs::read(temp.path().join("compiler.stdout"))
                    .unwrap()
                    .is_empty());
                assert!(fs::read(temp.path().join("compiler.stderr"))
                    .unwrap()
                    .is_empty());
            } else {
                assert_eq!(
                    fs::read(temp.path().join("compiler.stdout")).unwrap(),
                    b"out\xff"
                );
                assert_eq!(
                    fs::read(temp.path().join("compiler.stderr")).unwrap(),
                    b"err\r"
                );
                if expected_error {
                    assert_eq!(status["partial_streams_available"], true);
                }
            }
            if let Some(code) = expected_code {
                assert_eq!(status["code"], code);
            }
            if expected_error {
                assert!(status["error"].as_str().is_some_and(|s| !s.is_empty()));
            }
            assert!(temp.path().join("compiler-invocation.json").is_file());
        }
    }
    #[test]
    fn source_tree_rejects_links_and_specials_and_snapshot_detects_same_length_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("input");
        fs::write(&file, b"old").unwrap();
        let old = tree(temp.path(), 1, 3).unwrap();
        assert_eq!(old.len(), 1);
        fs::write(&file, b"new").unwrap();
        assert!(verify_map(&old).is_err());
        assert!(tree(temp.path(), 0, 3).is_err());
        assert!(tree(temp.path(), 1, 2).is_err());
        fs::remove_file(&file).unwrap();
        symlink("absent", &file).unwrap();
        assert!(tree(temp.path(), 1, 3).is_err());
        fs::remove_file(&file).unwrap();
        let _socket = std::os::unix::net::UnixListener::bind(&file).unwrap();
        assert!(tree(temp.path(), 1, 3).is_err());
    }
}

#[cfg(test)]
mod cached_binding_canonical_tests {
    use super::*;

    #[test]
    fn nested_insertion_order_does_not_change_or_authorize_a_cached_binding() {
        let first: Value = serde_json::from_str(r#"{"😀":"한글","helper":{"path":"/검증/😀/worker","sha256":"abc","identity":[1,2,3]},"가":"🌌","sources":[{"path":"원본/🚙.rs","sha256":"123"}]}"#).unwrap();
        let reordered: Value = serde_json::from_str(r#"{"sources":[{"sha256":"123","path":"원본/🚙.rs"}],"가":"🌌","helper":{"identity":[1,2,3],"sha256":"abc","path":"/검증/😀/worker"},"😀":"한글"}"#).unwrap();
        // Literal UTF8 bytes and independently pinned SHA define the shared
        // Python/Rust contract, including non-ASCII keys and astral characters.
        const EXPECTED_CANONICAL: &str = r#"{"helper":{"identity":[1,2,3],"path":"/검증/😀/worker","sha256":"abc"},"sources":[{"path":"원본/🚙.rs","sha256":"123"}],"가":"🌌","😀":"한글"}"#;
        const EXPECTED_SHA256: &str =
            "a20da5d9e880d168f27d7f9889b6b639b24caad3f1f86836cf57c6784e3e7ce8";
        let mut canonical = first.clone();
        canonical.sort_all_objects();
        assert_eq!(
            serde_json::to_vec(&canonical).unwrap(),
            EXPECTED_CANONICAL.as_bytes()
        );
        assert_eq!(cached_canonical_sha(&first).unwrap(), EXPECTED_SHA256);
        let expected = EXPECTED_SHA256;
        let before = serde_json::to_vec(&reordered).unwrap();
        assert_eq!(cached_canonical_sha(&reordered).unwrap(), expected);
        // Canonicalizing the digest must not mutate the issued receipt values.
        assert_eq!(serde_json::to_vec(&reordered).unwrap(), before);
        let mut changed = reordered;
        changed["helper"]["identity"][2] = json!(4);
        assert_ne!(cached_canonical_sha(&changed).unwrap(), expected);
        changed["helper"]["identity"][2] = json!(3);
        changed["sources"][0]["sha256"] = json!("stale");
        assert_ne!(cached_canonical_sha(&changed).unwrap(), expected);
    }
}

#[cfg(test)]
mod publisher_caller_tests {
    use super::*;
    #[test]
    fn publisher_receipt_requires_the_exact_held_issuer_and_both_observations() {
        let binding = json!({"enabled":true,"path":"/held/native","bytes":64,"sha256":"12".repeat(32),"identity":[1,2,64,493,5,6]});
        let good = json!({"catalog_publisher_before":binding,"catalog_publisher_after":binding});
        assert!(publisher_target(&good, Some(&binding)).is_ok());
        for field in ["catalog_publisher_before", "catalog_publisher_after"] {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(publisher_target(&missing, Some(&binding)).is_err());
            let mut replaced = good.clone();
            replaced[field]["identity"][1] = json!(3);
            assert!(publisher_target(&replaced, Some(&binding)).is_err());
        }
        assert!(publisher_target(&good, None).is_err());
        assert!(publisher_target(&json!({}), None).is_ok());
        assert!(publisher_target(&json!({"catalog_publisher_before":{"enabled":false},"catalog_publisher_after":{"enabled":false}}), None).is_err());
        let mut forged = good;
        forged["catalog_publisher_before"]["sha256"] = json!("00".repeat(32));
        forged["catalog_publisher_after"] = forged["catalog_publisher_before"].clone();
        assert!(publisher_target(&forged, Some(&binding)).is_err());
    }
}
