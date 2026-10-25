//! Continuous native IPC capture followed by separately supplied original trace.
//! Capture never writes a passing comparison receipt or launches an interpreter.
use openpilot_dev_tools::{
    controlsd_fixture_generate as fixture,
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
#[path = "controlsd_daemon_check/capture.rs"]
mod capture;
#[path = "controlsd_daemon_check/contract.rs"]
pub mod contract;
// Reuse the already supplied exact check_paramsd numeric/identity mechanisms.
#[path = "paramsd_estimator_check/identity.rs"]
mod identity;
#[path = "paramsd_estimator_check/numeric.rs"]
pub mod numeric;
use identity::{Bound, Directory};

const LIMIT: u64 = 32 * 1024 * 1024;
const PINS: [(&str, &str); 3] = [
    (
        "rust/tools/check_controlsd_daemon.py",
        "7c6e6286b381e2de48397b71b6aa343d4f6898b9cf91cf31e8218c52825f3493",
    ),
    (
        "rust/tools/controlsd_source.py",
        "f5a51d38c64197cc1f8786e8c10a3c5c0b54301deb8d958eae46cafcb321b7b0",
    ),
    (
        "rust/tools/check_paramsd.py",
        "5fda5495d04cdd665550f10a33bd61a3c3bc762e884a308f7bf152cbd10480a5",
    ),
];
pub fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn save(path: &Path, value: &Value) -> Result<()> {
    create(path, &serde_json::to_vec_pretty(value)?)
}
fn load(path: &Path) -> Result<Value> {
    numeric::parse_python(&Bound::open(path, None)?.bytes(LIMIT)?)
}
fn check(bounds: &[Bound]) -> Result<()> {
    for held in bounds {
        held.verify()?;
    }
    Ok(())
}
fn pins(bounds: &[Bound]) -> Value {
    json!(bounds
        .iter()
        .map(|b| json!({"path":b.path,"sha256":b.hash}))
        .collect::<Vec<_>>())
}
fn tree(path: &Path, bounds: &mut Vec<Bound>) -> Result<()> {
    let mut paths = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)?;
        require(
            !metadata.file_type().is_symlink(),
            "provider/asset tree contains an alias",
        )?;
        if metadata.is_dir() {
            tree(&path, bounds)?;
        } else {
            require(
                metadata.is_file() && metadata.len() <= 256 * 1024 * 1024,
                "provider/asset is not bounded regular data",
            )?;
            require(
                bounds.len() < 512,
                "provider/source descriptor roster exceeds 512",
            )?;
            bounds.push(Bound::open(&path, None)?);
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub daemon: PathBuf,
    pub numerics: PathBuf,
    pub assets: PathBuf,
    pub evidence: PathBuf,
    pub original: Option<PathBuf>,
    pub source_commit: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    root: PathBuf,
    daemon: PathBuf,
    numerics: PathBuf,
    assets: PathBuf,
    output: PathBuf,
    params: PathBuf,
    shm: PathBuf,
    signal: i32,
    saved: BTreeMap<String, String>,
}
struct Signals {
    stop: Arc<AtomicBool>,
    ids: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let mut result = Self {
            stop: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        for s in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            result
                .ids
                .push(signal_hook::flag::register(s, result.stop.clone())?);
        }
        Ok(result)
    }
    fn check(&self) -> Result<()> {
        require(
            !self.stop.load(std::sync::atomic::Ordering::SeqCst),
            "controlsd daemon controller cancelled",
        )
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
fn source_bounds(options: &Options) -> Result<Vec<Bound>> {
    fixture::verify_sources(&options.root)?;
    let mut bounds = Vec::new();
    for (name, pin) in PINS {
        bounds.push(Bound::open(&options.root.join(name), Some(pin))?);
    }
    for name in [
        "rust/Cargo.lock",
        "rust/crates/dev-tools/Cargo.toml",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/dev-tools/src/controlsd_fixture_generate.rs",
        "rust/crates/dev-tools/src/controlsd_daemon_check.rs",
        "rust/crates/dev-tools/src/controlsd_daemon_check/capture.rs",
        "rust/crates/dev-tools/src/controlsd_daemon_check/contract.rs",
        "rust/crates/dev-tools/src/bin/check_controlsd_daemon_native.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/identity.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/numeric.rs",
        "openpilot/cereal/log.capnp",
        "opendbc_repo/opendbc/car/car.capnp",
    ] {
        bounds.push(Bound::open(&options.root.join(name), None)?);
    }
    for (name, pin) in fixture::SOURCES {
        bounds.push(Bound::open(&options.root.join(name), Some(pin))?);
    }
    let mut daemon = Bound::open(&options.daemon, None)?;
    daemon.native_elf()?;
    bounds.push(daemon);
    let mut executable = Bound::open(&std::env::current_exe()?, None)?;
    executable.native_elf()?;
    bounds.push(executable);
    tree(&options.numerics, &mut bounds)?;
    tree(&options.assets, &mut bounds)?;
    tree(
        &options
            .root
            .join("rust/crates/dev-tools/src/controlsd_fixture_generate"),
        &mut bounds,
    )?;
    Ok(bounds)
}
pub fn capture(options: &Options) -> Result<()> {
    let signals = Signals::new()?;
    let bounds = source_bounds(options)?;
    let directory = Directory::create(&options.evidence)?;
    let evidence = options.evidence.canonicalize()?;
    save(&evidence.join("bindings-before.json"), &pins(&bounds))?;
    let mut saved = BTreeMap::new();
    let mut phases = serde_json::Map::new();
    for signal in [2, 15] {
        signals.check()?;
        check(&bounds)?;
        directory.verify()?;
        let output = evidence.join(signal.to_string());
        fs::create_dir(&output)?;
        let private = tempfile::tempdir()?;
        let shm = tempfile::Builder::new()
            .prefix("msgq_controlsd-daemon-")
            .tempdir_in("/dev/shm")?;
        let params = private.path().join("params");
        fs::create_dir(&params)?;
        let request = Request {
            root: options.root.clone(),
            daemon: options.daemon.clone(),
            numerics: options.numerics.clone(),
            assets: options.assets.clone(),
            output: output.clone(),
            params,
            shm: shm.path().to_owned(),
            signal,
            saved,
        };
        let request_path = output.join("request.json");
        save(&request_path, &serde_json::to_value(request)?)?;
        let request_bound = Bound::open(&request_path, None)?;
        let mut command = CommandSpec::new_os(
            vec![
                std::env::current_exe()?.into_os_string(),
                "__phase".into(),
                request_path.into_os_string(),
            ],
            Duration::from_secs(180),
        );
        command.cwd = Some(options.root.clone());
        command.stdout_limit = 1024 * 1024;
        command.stderr_limit = 1024 * 1024;
        let result =
            NativeRunner.run_controlled(&command, Instant::now() + command.timeout, &signals.stop);
        match result {
            Ok(result) => {
                create(&output.join("worker.stdout"), &result.stdout)?;
                create(&output.join("worker.stderr"), &result.stderr)?;
                save(&output.join("process.json"), &json!({"exit":result.code}))?;
                require(result.code == 0, "controlsd capture worker failed")?;
            }
            Err(error) => {
                save(
                    &output.join("process.json"),
                    &json!({"error":error.to_string()}),
                )?;
                return Err(error);
            }
        }
        request_bound.verify()?;
        saved = serde_json::from_value(load(&output.join("persisted.json"))?)?;
        contract::persisted(&saved)?;
        let mut members = serde_json::Map::new();
        for name in [
            "request.json",
            "input.json",
            "packets.json",
            "facts.json",
            "persisted.json",
            "maps.txt",
            "cmdline.bin",
            "reader-readiness.json",
        ] {
            let file = Bound::open(&output.join(name), None)?;
            require(
                file.bytes(LIMIT)?.len() as u64 <= LIMIT,
                "phase member exceeds limit",
            )?;
            members.insert(name.into(), json!(file.hash));
        }
        phases.insert(signal.to_string(), Value::Object(members));
        // The outer owner retains both directories until its entire worker group
        // has been terminated/reaped, including every worker failure path.
        drop(shm);
        drop(private);
    }
    signals.check()?;
    check(&bounds)?;
    directory.verify()?;
    save(&evidence.join("bindings-after.json"), &pins(&bounds))?;
    save(
        &evidence.join("capture.json"),
        &json!({"schema":1,"source_commit":options.source_commit,"root":options.root,"daemon":options.daemon,"phases":phases,"bindings":pins(&bounds),"comparison_pending":true}),
    )?;
    Ok(())
}
fn bound_members(base: &Path, members: &Value, bounds: &mut Vec<Bound>) -> Result<()> {
    for (name, digest) in members.as_object().ok_or("member seal object required")? {
        require(
            Path::new(name).components().count() == 1 && name != "." && name != "..",
            "nonlocal member name",
        )?;
        bounds.push(Bound::open(
            &base.join(name),
            Some(digest.as_str().ok_or("member digest required")?),
        )?);
    }
    Ok(())
}
fn member_names(members: &Value, expected: &[&str]) -> Result<()> {
    let mut actual = members
        .as_object()
        .ok_or("member seal object required")?
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    require(actual == expected, "sealed phase member roster differs")
}
pub fn compare(options: &Options) -> Result<()> {
    let signals = Signals::new()?;
    let original = options
        .original
        .as_ref()
        .ok_or("--original required for compare")?
        .canonicalize()?;
    let mut bounds = source_bounds(options)?;
    let evidence = options.evidence.canonicalize()?;
    let evidence_identity = fs::metadata(&evidence)?;
    match fs::symlink_metadata(evidence.join("results.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err("foreign comparison receipt already exists or cannot be checked".into()),
    }
    let native_path = options.evidence.join("capture.json");
    let native_bound = Bound::open(&native_path, None)?;
    let native = numeric::parse_python(&native_bound.bytes(LIMIT)?)?;
    let original_bound = Bound::open(&original.join("capture.json"), None)?;
    let reference = numeric::parse_python(&original_bound.bytes(LIMIT)?)?;
    require(
        native["schema"] == 1 && reference["schema"] == 1 && native["comparison_pending"] == true,
        "capture schema differs",
    )?;
    member_names(&native["phases"], &["2", "15"])?;
    member_names(&reference["phases"], &["2", "15"])?;
    require(
        native["source_commit"] == options.source_commit
            && reference["source_commit"] == options.source_commit
            && native["root"] == json!(options.root)
            && reference["native_capture_sha256"] == native_bound.hash,
        "original/native source binding differs",
    )?;
    require(
        native["bindings"] == pins(&bounds),
        "current capture source/provider/ELF bindings differ",
    )?;
    for binding in reference["bindings"]
        .as_array()
        .ok_or("original provider/source roster required")?
    {
        let path = PathBuf::from(binding["path"].as_str().ok_or("original binding path")?);
        require(path.is_absolute(), "relative original provider binding")?;
        bounds.push(Bound::open(
            &path,
            Some(binding["sha256"].as_str().ok_or("original binding hash")?),
        )?);
    }
    bounds.push(native_bound);
    bounds.push(original_bound);
    let mut results = Vec::new();
    for signal in [2, 15] {
        let name = signal.to_string();
        let phase = options.evidence.join(&name);
        let source = original.join(&name);
        member_names(
            &native["phases"][&name],
            &[
                "request.json",
                "input.json",
                "packets.json",
                "facts.json",
                "persisted.json",
                "maps.txt",
                "cmdline.bin",
                "reader-readiness.json",
            ],
        )?;
        member_names(
            &reference["phases"][&name],
            &[
                "input.json",
                "source.json",
                "original-wire-input.json",
                "input-parity.json",
            ],
        )?;
        bound_members(&phase, &native["phases"][&name], &mut bounds)?;
        bound_members(&source, &reference["phases"][&name], &mut bounds)?;
        let parity = load(&source.join("input-parity.json"))?;
        require(
            parity["pass"] == true && parity["exact_full_schema_float_bits"] == true,
            "independent input parity did not pass",
        )?;
        let input = load(&phase.join("input.json"))?;
        require(
            input == load(&source.join("input.json"))?,
            "independent original input differs",
        )?;
        let packets: Vec<Vec<Vec<u8>>> =
            serde_json::from_value(load(&phase.join("packets.json"))?)?;
        let facts = load(&phase.join("facts.json"))?;
        let settings: BTreeMap<String, String> =
            serde_json::from_value(load(&phase.join("persisted.json"))?)?;
        contract::persisted(&settings)?;
        contract::observations(&input, &packets, &facts, signal, &options.daemon)?;
        let rows = load(&source.join("source.json"))?;
        require(
            rows["name"] == input["name"],
            "original source phase name differs",
        )?;
        let maps = fs::read_to_string(phase.join("maps.txt"))?;
        require(
            !["libpython", "numpy/_core", "cython"]
                .iter()
                .any(|name| maps.contains(name))
                && maps.contains("libscipy_openblas"),
            "retained runtime mappings differ",
        )?;
        use std::os::unix::ffi::OsStrExt;
        let cmdline = fs::read(phase.join("cmdline.bin"))?;
        let argv = cmdline.split(|b| *b == 0).collect::<Vec<_>>();
        require(
            argv == [
                options.daemon.as_os_str().as_bytes(),
                b"--numerics",
                options.numerics.as_os_str().as_bytes(),
                b"--assets",
                options.assets.as_os_str().as_bytes(),
                b"",
            ],
            "retained daemon argv differs",
        )?;
        let stats = contract::compare_rows(&rows, &packets)?;
        require(stats.numeric_comparisons > 0, "empty numeric comparison")?;
        let mut result = facts;
        let fields = result.as_object_mut().ok_or("phase facts object")?;
        fields.insert("pass".into(), json!(true));
        fields.insert("source_equal".into(), json!(true));
        fields.insert("max_error".into(), json!(stats.max_absolute_error));
        results.push(result);
    }
    signals.check()?;
    check(&bounds)?;
    // Exclusive final receipt is written only after every phase, source/ELF/
    // provider/observation seal and complete original comparison succeeded.
    let mut stdout = std::io::stdout().lock();
    for result in &results {
        writeln!(stdout, "{}", serde_json::to_string(result)?)?;
    }
    stdout.flush()?;
    drop(stdout);
    signals.check()?;
    use std::os::unix::fs::MetadataExt;
    let receipt_path = evidence.join("results.json");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt_path)?;
    let mut receipt = Receipt {
        path: receipt_path,
        file,
        committed: false,
    };
    receipt
        .file
        .write_all(&serde_json::to_vec_pretty(&results)?)?;
    receipt.file.sync_all()?;
    signals.check()?;
    check(&bounds)?;
    let now = fs::metadata(&evidence)?;
    require(
        (now.dev(), now.ino()) == (evidence_identity.dev(), evidence_identity.ino())
            && receipt.owns(),
        "comparison evidence/receipt replaced",
    )?;
    File::open(&evidence)?.sync_all()?;
    signals.check()?;
    receipt.committed = true;
    Ok(())
}
struct Receipt {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Receipt {
    fn owns(&self) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.file.metadata().is_ok_and(|held| {
            fs::symlink_metadata(&self.path).is_ok_and(|now| {
                now.is_file() && (held.dev(), held.ino()) == (now.dev(), now.ino())
            })
        })
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        if !self.committed && self.owns() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().ok_or("capture or compare required")?;
    if mode == "__phase" {
        let path = PathBuf::from(args.next().ok_or("worker request required")?);
        require(args.next().is_none(), "extra worker arguments")?;
        let bound = Bound::open(&path, None)?;
        let request: Request = serde_json::from_slice(&bound.bytes(64 * 1024)?)?;
        capture::phase(&request)?;
        return bound.verify();
    }
    require(
        mode == "capture" || mode == "compare",
        "unknown controller mode",
    )?;
    let mut values = BTreeMap::new();
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| "non-UTF-8 option")?;
        require(
            [
                "--root",
                "--daemon",
                "--numerics",
                "--assets",
                "--evidence",
                "--original",
                "--source-commit",
            ]
            .contains(&flag.as_str()),
            "unknown option",
        )?;
        let value = args.next().ok_or("missing option value")?;
        require(values.insert(flag, value).is_none(), "duplicate option")?;
    }
    let original = values.remove("--original").map(PathBuf::from);
    let mut take = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("{name} required"))
    };
    let root = PathBuf::from(take("--root")?).canonicalize()?;
    let evidence = PathBuf::from(take("--evidence")?);
    let source_commit = take("--source-commit")?
        .into_string()
        .map_err(|_| "non-UTF-8 commit")?;
    require(
        source_commit.len() == 40
            && source_commit
                .bytes()
                .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x)),
        "invalid source commit",
    )?;
    let options = Options {
        root,
        evidence,
        original,
        source_commit,
        daemon: PathBuf::from(take("--daemon")?).canonicalize()?,
        numerics: PathBuf::from(take("--numerics")?).canonicalize()?,
        assets: PathBuf::from(take("--assets")?).canonicalize()?,
    };
    if mode == "capture" {
        require(
            options.original.is_none(),
            "capture cannot accept an oracle backend",
        )?;
        capture(&options)
    } else {
        compare(&options)
    }
}
