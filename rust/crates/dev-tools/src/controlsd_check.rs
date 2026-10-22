//! Bounded native replacement for check_controlsd.py/controlsd_compare.py.
//! The independently captured original trace is data, never an executable backend.
use openpilot_dev_tools::{
    controlsd_fixture_generate as fixture,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
// Reuse exactly the accepted comparator, Event reader and ownership source.
// These are also included by the existing paramsd binary; no divergent copy.
#[allow(dead_code)]
#[path = "paramsd_estimator_check/reference.rs"]
mod event_reference;
#[path = "paramsd_estimator_check/identity.rs"]
mod identity;
#[allow(dead_code)]
#[path = "paramsd_estimator_check/numeric.rs"]
pub mod numeric;
#[path = "controlsd_check/reference.rs"]
pub mod reference;
use identity::{Bound, Directory};
const LIMIT: u64 = 512 * 1024 * 1024;
pub const TOKENS: [&str; 10] = [
    "1_0",
    "0x_A",
    "0Xf_f",
    "0b_10",
    "0o_10",
    "0_0",
    "00",
    "- 1",
    "+ 1",
    "-2147483648",
];
const PINS: [(&str, &str); 3] = [
    (
        "rust/tools/check_controlsd.py",
        "0ea4b8dd0108b8f96955c24febb4ea27206ddda0860d757aed2ca85bf4df2749",
    ),
    (
        "rust/tools/controlsd_compare.py",
        "52c4b064257fc09c9ff85c38ddd0e70b07d9572ad9cb61e2a46dc5c5a7fac830",
    ),
    (
        "rust/tools/controlsd_source.py",
        "f5a51d38c64197cc1f8786e8c10a3c5c0b54301deb8d958eae46cafcb321b7b0",
    ),
];
pub const SOURCE_DIRS: [&str; 7] = [
    "rust/crates",
    "openpilot/cereal",
    "openpilot/common",
    "openpilot/selfdrive/controls",
    "openpilot/selfdrive/carrot",
    "opendbc_repo/opendbc/car",
    "rust/tools",
];
pub const SOURCE_FILES: [&str; 5] = [
    "uv.lock",
    "rust/Cargo.lock",
    "rust/Cargo.toml",
    "openpilot/selfdrive/locationd/helpers.py",
    "openpilot/selfdrive/modeld/constants.py",
];
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
/// Complete conservative source closure; excludes unrelated new dev-tools callers.
/// Imported project modules outside this closure are added by original capture.
pub fn source_paths(root: &Path) -> Result<BTreeSet<PathBuf>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if entry.file_name() == "__pycache__" || path == root.join("rust/crates/dev-tools")
                {
                    continue;
                }
                visit(root, &path, out)?;
            } else if kind.is_file()
                && path.extension().and_then(|x| x.to_str()).is_some_and(|e| {
                    [
                        "rs", "toml", "json", "capnp", "py", "pyx", "pxd", "h", "c", "cc", "cpp",
                        "dbc", "yaml",
                    ]
                    .contains(&e)
                })
            {
                out.insert(path.strip_prefix(root)?.into());
            } else if kind.is_symlink() {
                return Err(format!("source closure symlink: {}", path.display()).into());
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    for dir in SOURCE_DIRS {
        visit(root, &root.join(dir), &mut out)?;
    }
    for name in SOURCE_FILES {
        out.insert(name.into());
    }
    for (name, _) in PINS.into_iter().chain(fixture::SOURCES) {
        out.insert(name.into());
    }
    // The shared decoder/comparator and fixture implementation are source-bound.
    for name in [
        "paramsd_estimator_check/numeric.rs",
        "paramsd_estimator_check/reference.rs",
        "paramsd_fixture_generate.rs",
        "journald_check.rs",
        "controlsd_fixture_generate.rs",
        "controlsd_fixture_generate/audit.rs",
        "controlsd_fixture_generate/recipes.rs",
        "controlsd_fixture_generate/parameters.rs",
        "controlsd_fixture_generate/wire.rs",
    ] {
        out.insert(Path::new("rust/crates/dev-tools/src").join(name));
    }
    Ok(out)
}
pub fn extend_fingerprint_cases(cases: &mut Vec<Value>) -> Result<()> {
    let base = cases
        .get(3)
        .ok_or("base fixture case three missing")?
        .clone();
    for token in TOKENS {
        let mut case = base.clone();
        case["name"] = json!(format!("fingerprints-valid-{token}"));
        case.get_mut("frames")
            .and_then(Value::as_array_mut)
            .ok_or("base frames missing")?
            .truncate(5);
        let params = case
            .get_mut("params")
            .and_then(Value::as_object_mut)
            .ok_or("base params missing")?;
        params.insert(
            "FingerPrints".into(),
            json!(format!("{{0: {{{token}: 8}}, 1: {{}}, 2: {{}}, 3: {{}}}}").into_bytes()),
        );
        cases.push(case);
    }
    Ok(())
}
pub fn request() -> Result<Value> {
    let generated = fixture::generate(&JsonValue::parse(r#"{"op":"cases"}"#)?)?;
    let mut cases: Vec<Value> = serde_json::from_str(&generated.to_json()?)?;
    require(cases.len() == 30, "fixture base/scenario roster changed")?;
    extend_fingerprint_cases(&mut cases)?;
    Ok(json!({"cases":cases}))
}
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub original: PathBuf,
    pub numerics: PathBuf,
    pub python_prefix: PathBuf,
    pub evidence: PathBuf,
    pub source_commit: String,
    pub capture_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provider {
    base: String,
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    schema: u32,
    origin: String,
    source_commit: String,
    source_files: BTreeMap<PathBuf, String>,
    providers: Vec<Provider>,
    members: BTreeMap<String, String>,
    numerics: BTreeMap<PathBuf, String>,
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
fn verify(stop: &AtomicBool, dir: &Directory, bounds: &[Bound]) -> Result<()> {
    require(
        !stop.load(Ordering::SeqCst),
        "controlsd controller cancelled",
    )?;
    dir.verify()?;
    for bound in bounds {
        bound.verify()?;
    }
    Ok(())
}
fn bind(bounds: &mut Vec<Bound>, path: &Path, hash: &str) -> Result<()> {
    require(hex(hash, 64), "invalid capture digest")?;
    bounds.push(Bound::open(path, Some(hash))?);
    Ok(())
}
/// Runner injection exercises ownership failures. Production uses NativeRunner only.
pub fn execute(options: &Options, runner: &mut dyn Runner, stop: &AtomicBool) -> Result<Value> {
    let fsize = rustix::process::getrlimit(rustix::process::Resource::Fsize);
    require(
        fsize.current.is_some_and(|n| n <= LIMIT),
        "controller process needs a file-size limit <=512 MiB",
    )?;
    require(
        hex(&options.source_commit, 40) && hex(&options.capture_sha256, 64),
        "invalid source/capture identity",
    )?;
    require(
        !stop.load(Ordering::SeqCst),
        "controlsd controller cancelled",
    )?;
    let root = options.root.canonicalize()?;
    let original = options.original.canonicalize()?;
    let numerics = options.numerics.canonicalize()?;
    let python_prefix = options.python_prefix.canonicalize()?;
    let mut bounds = vec![Bound::open(
        &original.join("capture.json"),
        Some(&options.capture_sha256),
    )?];
    let capture: Capture = serde_json::from_slice(&bounds[0].bytes(16 * 1024 * 1024)?)?;
    require(
        capture.schema == 1
            && capture.origin == "unchanged-controlsd-source-40"
            && hex(&capture.source_commit, 40),
        "original provenance contract differs",
    )?;
    require(
        capture.source_files.len() + capture.providers.len() < 16384,
        "capture binding limit exceeded",
    )?;
    let required = source_paths(&root)?;
    require(
        required
            .iter()
            .all(|p| capture.source_files.contains_key(p)),
        "original source closure incomplete; obtain source-admitted capture",
    )?;
    for (path, hash) in &capture.source_files {
        require(relative(path), "source path escaped root")?;
        bind(&mut bounds, &root.join(path), hash)?;
    }
    for (name, pin) in PINS.into_iter().chain(fixture::SOURCES) {
        require(
            capture
                .source_files
                .get(Path::new(name))
                .is_some_and(|h| h == pin),
            "original caller/fixture pin differs",
        )?;
    }
    require(
        !capture.providers.is_empty(),
        "original provider closure missing",
    )?;
    for p in &capture.providers {
        let path = match p.base.as_str() {
            "python" => {
                require(relative(&p.path), "provider escaped Python prefix")?;
                python_prefix.join(&p.path)
            }
            "absolute" => {
                require(p.path.is_absolute(), "absolute provider must be absolute")?;
                p.path.clone()
            }
            _ => return Err("unknown provider base".into()),
        };
        bind(&mut bounds, &path, &p.sha256)?;
    }
    // The exact staged numerical manifest/files used by original NumPy are sealed.
    require(
        capture.numerics.contains_key(Path::new("manifest.json")),
        "missing numerical manifest seal",
    )?;
    for (path, hash) in &capture.numerics {
        require(
            relative(path) && path.components().count() == 1,
            "numerics path escaped directory",
        )?;
        bind(&mut bounds, &numerics.join(path), hash)?;
    }
    let manifest_bound = Bound::open(&numerics.join("manifest.json"), None)?;
    let manifest: Value = serde_json::from_slice(&manifest_bound.bytes(1024 * 1024)?)?;
    require(
        manifest["format"] == 1
            && manifest["numpy"] == "2.5.3"
            && manifest["abi"] == "scipy_dgesdd_64_",
        "exact CONTROLS_NUMERICS contract required",
    )?;
    let files = manifest["files"]
        .as_array()
        .ok_or("numerical file roster missing")?;
    require(
        files.len() + 1 == capture.numerics.len(),
        "numerics manifest/file seal roster differs",
    )?;
    for file in files {
        let name = file["name"].as_str().ok_or("numerics filename missing")?;
        require(
            capture
                .numerics
                .get(Path::new(name))
                .is_some_and(|h| Some(h.as_str()) == file["sha256"].as_str()),
            "numerics file hash differs",
        )?;
    }
    require(
        files.iter().any(|f| f["name"] == manifest["library"]),
        "numerical library not bound",
    )?;
    let mut values = BTreeMap::new();
    require(
        capture.members.len() == 2,
        "capture must contain input/source only",
    )?;
    for name in ["input.json", "source.json"] {
        let hash = capture
            .members
            .get(name)
            .ok_or("original member seal missing")?;
        bind(&mut bounds, &original.join(name), hash)?;
        let raw = bounds.last().ok_or("missing bound member")?.bytes(LIMIT)?;
        values.insert(name, (numeric::parse_python(&raw)?, raw));
    }
    require(
        !fs::symlink_metadata(&options.binary)?
            .file_type()
            .is_symlink(),
        "native trace symlink forbidden",
    )?;
    let binary = options.binary.canonicalize()?;
    let mut trace = Bound::open(&binary, None)?;
    trace.native_elf()?;
    bounds.push(trace);
    let directory = Directory::create(&options.evidence)?;
    let evidence = options.evidence.canonicalize()?;
    // A fresh evidence root prevents overwriting foreign inputs, output or receipts.
    let private = tempfile::Builder::new()
        .prefix("owned-trace-")
        .tempdir_in(&evidence)?;
    let output = private.path().join("native-raw.json");
    verify(stop, &directory, &bounds)?;
    let input = request()?;
    let canonical = |v: &Value| -> Result<Value> {
        fixture::canonical_inputs(&JsonValue::parse(&serde_json::to_string(v)?)?)
    };
    require(
        canonical(&input)? == canonical(&values["input.json"].0)?,
        "generated caller input differs from full-schema original input",
    )?;
    let cases = input["cases"].as_array().ok_or("generated cases missing")?;
    let expected = values["source.json"]
        .0
        .as_array()
        .ok_or("original cases missing")?;
    require(
        cases.len() == 40 && expected.len() == cases.len(),
        "40 original cases required",
    )?;
    for (case, old) in cases.iter().zip(expected) {
        require(
            case["name"] == old["name"]
                && case["frames"].as_array().map(Vec::len) == old["rows"].as_array().map(Vec::len),
            "original case/frame order differs",
        )?;
    }
    create(&evidence.join("input.json"), &serde_json::to_vec(&input)?)?;
    create(&evidence.join("source.json"), &values["source.json"].1)?;
    let mut spec = CommandSpec::new_os(
        vec![binary.into_os_string(), output.clone().into_os_string()],
        Duration::from_secs(600),
    );
    spec.cwd = Some(root.clone());
    spec.input = Some(serde_json::to_vec(&input)?);
    spec.env.insert(
        "CONTROLS_NUMERICS".into(),
        numerics
            .to_str()
            .ok_or("numerics path UTF-8 required")?
            .into(),
    );
    spec.env.insert(
        "CONTROLS_ASSETS".into(),
        root.join("opendbc_repo/opendbc/car/torque_data")
            .to_str()
            .ok_or("asset path UTF-8 required")?
            .into(),
    );
    spec.env.insert("PATH".into(), "/nonexistent".into());
    spec.stdout_limit = 8 * 1024 * 1024;
    spec.stderr_limit = 8 * 1024 * 1024;
    spec.disable_core_dump = true;
    // CLI-owned RLIMIT_FSIZE is inherited by this child; parent shell unchanged.
    verify(stop, &directory, &bounds)?;
    let process = runner.run(&spec);
    if let Ok(child) = &process {
        let mut log = child.stdout.clone();
        log.extend(&child.stderr);
        log.extend(format!("\nEXIT {}\n", child.code).bytes());
        create(&evidence.join("process.log"), &log)?;
    }
    verify(stop, &directory, &bounds)?;
    let child = process?;
    require(
        child.code == 0,
        &format!("native controlsd trace exited {}", child.code),
    )?;
    let raw = Bound::open(&output, None)?;
    let raw_bytes = raw.bytes(LIMIT)?;
    bounds.push(raw);
    create(&evidence.join("native-raw.json"), &raw_bytes)?;
    let mut actual = numeric::parse_python(&raw_bytes)?;
    reference::normalize(&mut actual)?;
    let (actual, result) = reference::compare(&values["source.json"].0, actual)?;
    create(
        &evidence.join("native.json"),
        &reference::python_json(&actual)?,
    )?;
    verify(stop, &directory, &bounds)?;
    create(
        &evidence.join("admission.json"),
        &serde_json::to_vec_pretty(
            &json!({"current_source_commit":options.source_commit,"original_source_commit":capture.source_commit,"capture_sha256":options.capture_sha256,"bindings":bounds.iter().map(|b|json!({"path":b.path,"sha256":b.hash})).collect::<Vec<_>>(),"argv":spec.argv,"numerics":numerics,"timeout_seconds":600,"original_backend_execs":0}),
        )?,
    )?;
    // Passing receipt is last, atomic and removed on post-write cancellation/error.
    let receipt = evidence.join("results.json");
    let pending = private.path().join("results.pending.json");
    create(&pending, &serde_json::to_vec_pretty(&result)?)?;
    verify(stop, &directory, &bounds)?;
    fs::hard_link(&pending, &receipt)?;
    if let Err(error) = verify(stop, &directory, &bounds)
        .and_then(|_| File::open(&evidence)?.sync_all().map_err(Into::into))
    {
        let a = fs::symlink_metadata(&receipt)?;
        let b = fs::metadata(&pending)?;
        use std::os::unix::fs::MetadataExt;
        if (a.dev(), a.ino()) == (b.dev(), b.ino()) {
            fs::remove_file(receipt)?;
        }
        return Err(error);
    }
    Ok(result)
}
struct Signals {
    stop: Arc<AtomicBool>,
    ids: Vec<signal_hook::SigId>,
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn run_owned(spec: &CommandSpec, stop: &AtomicBool) -> Result<Captured> {
    NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, stop)
}
struct Controlled(Arc<AtomicBool>);
impl Runner for Controlled {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        run_owned(spec, &self.0)
    }
}
pub fn cli() -> Result<()> {
    let limit = rustix::process::getrlimit(rustix::process::Resource::Fsize);
    rustix::process::setrlimit(
        rustix::process::Resource::Fsize,
        rustix::process::Rlimit {
            current: Some(limit.current.unwrap_or(LIMIT).min(LIMIT)),
            maximum: limit.maximum,
        },
    )?;
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| "option UTF-8 required")?;
        require(
            [
                "--root",
                "--trace",
                "--original",
                "--numerics",
                "--python-prefix",
                "--evidence",
                "--source-commit",
                "--capture-sha256",
            ]
            .contains(&flag.as_str()),
            "unknown controlsd option",
        )?;
        let value = args.next().ok_or("option value missing")?;
        require(
            values.insert(flag, value).is_none(),
            "duplicate controlsd option",
        )?;
    }
    let mut take = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("{name} required"))
    };
    let options = Options {
        root: take("--root")?.into(),
        binary: take("--trace")?.into(),
        original: take("--original")?.into(),
        numerics: take("--numerics")?.into(),
        python_prefix: take("--python-prefix")?.into(),
        evidence: take("--evidence")?.into(),
        source_commit: take("--source-commit")?
            .into_string()
            .map_err(|_| "source UTF-8 required")?,
        capture_sha256: take("--capture-sha256")?
            .into_string()
            .map_err(|_| "capture UTF-8 required")?,
    };
    let mut signals = Signals {
        stop: Arc::new(AtomicBool::new(false)),
        ids: Vec::new(),
    };
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .ids
            .push(signal_hook::flag::register(sig, Arc::clone(&signals.stop))?);
    }
    let result = execute(
        &options,
        &mut Controlled(Arc::clone(&signals.stop)),
        &signals.stop,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
