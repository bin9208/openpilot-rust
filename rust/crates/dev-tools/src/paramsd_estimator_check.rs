//! Native process/comparison controller for the retained check_paramsd.py.
//! The original learner/oracle remains independent and must run first in Actions.
use openpilot_dev_tools::{
    paramsd_fixture_generate,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[path = "paramsd_estimator_check/identity.rs"]
mod identity;
#[path = "paramsd_estimator_check/numeric.rs"]
pub mod numeric;
#[path = "paramsd_estimator_check/reference.rs"]
pub mod reference;
use identity::{Bound, Directory};
const LIMIT: u64 = 32 * 1024 * 1024;
const PINS: &[(&str, &str)] = &[
    (
        "rust/tools/check_paramsd.py",
        "5fda5495d04cdd665550f10a33bd61a3c3bc762e884a308f7bf152cbd10480a5",
    ),
    (
        "rust/tools/paramsd_source.py",
        "2df7fab34e90945d2c5d8434bba8ff44ce1e6e184a19b9da7ab0679c761d2a5f",
    ),
    (
        "rust/tools/paramsd_fixture.py",
        "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521",
    ),
    (
        "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
        "cc7a6811f29d51f4d2662572176f12551aec1f7b56922ca67cd62d2d36bb46ac",
    ),
];
const REQUIRED_SOURCE: &[&str] = &[
    "rust/Cargo.lock",
    "openpilot/selfdrive/locationd/paramsd.py",
    "openpilot/selfdrive/locationd/helpers.py",
    "openpilot/selfdrive/locationd/models/car_kf.py",
    "openpilot/selfdrive/locationd/models/constants.py",
    "openpilot/common/transformations/orientation.py",
    "openpilot/common/transformations/transformations.py",
    "openpilot/cereal/log.capnp",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/deprecated.capnp",
    "opendbc_repo/opendbc/car/car.capnp",
    "openpilot/cereal/messaging/__init__.py",
    "rednose_repo/rednose/helpers/kalmanfilter.py",
    "rednose_repo/rednose/helpers/ekf_sym_pyx.pyx",
    "rust/crates/paramsd/examples/paramsd_trace.rs",
];
#[cfg(test)]
pub fn required_source_for_test() -> &'static [&'static str] {
    REQUIRED_SOURCE
}
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub original: PathBuf,
    pub oracle: PathBuf,
    pub evidence: PathBuf,
    pub source_commit: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    schema: u32,
    source_commit: String,
    root: PathBuf,
    oracle: PathBuf,
    binary: Binding,
    members: BTreeMap<String, String>,
    bindings: Vec<Binding>,
}
fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
}
fn require(ok: bool, message: &str) -> Result<()> {
    if !ok {
        return Err(message.into());
    }
    Ok(())
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn json_file(path: &Path, value: &Value) -> Result<()> {
    create(path, &serde_json::to_vec_pretty(value)?)
}
fn check(stop: &AtomicBool, directory: &Directory, bounds: &[Bound]) -> Result<()> {
    require(
        !stop.load(Ordering::SeqCst),
        "paramsd estimator controller cancelled",
    )?;
    directory.verify()?;
    for bound in bounds {
        bound.verify()?;
    }
    Ok(())
}
fn bindings(bounds: &[Bound]) -> Value {
    json!(bounds
        .iter()
        .map(|b| json!({"path":b.path,"sha256":b.hash}))
        .collect::<Vec<_>>())
}
struct Receipt {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Receipt {
    fn owns(&self) -> bool {
        self.file.metadata().ok().is_some_and(|held| {
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

/// Runner injection is for owned-process contract tests. Expected functional
/// results still come only from original/source.json admitted by capture.json.
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    stop: &AtomicBool,
    printed: &mut dyn Write,
) -> Result<Value> {
    require(
        hex(&options.source_commit, 40),
        "invalid estimator source commit",
    )?;
    let root = options.root.canonicalize()?;
    let original = options.original.canonicalize()?;
    let oracle = options.oracle.canonicalize()?;
    require(
        !fs::symlink_metadata(&options.binary)?
            .file_type()
            .is_symlink(),
        "native estimator binary is a symlink",
    )?;
    let binary = options.binary.canonicalize()?;
    let mut bounds = vec![Bound::open(&original.join("capture.json"), None)?];
    let capture: Capture = serde_json::from_slice(&bounds[0].bytes(LIMIT)?)?;
    require(
        capture.schema == 1
            && capture.source_commit == options.source_commit
            && capture.root == root
            && capture.oracle == oracle,
        "fresh original capture source/root/oracle differs",
    )?;
    require(
        capture.binary.path == binary && hex(&capture.binary.sha256, 64),
        "fresh original/native ELF binding differs",
    )?;
    let mut native = Bound::open(&binary, Some(&capture.binary.sha256))?;
    native.native_elf()?;
    bounds.push(native);
    let mut admitted = BTreeSet::new();
    for binding in &capture.bindings {
        require(
            binding.path.is_absolute()
                && hex(&binding.sha256, 64)
                && admitted.insert(binding.path.clone()),
            "invalid/duplicate original source/provider binding",
        )?;
        bounds.push(Bound::open(&binding.path, Some(&binding.sha256))?);
    }
    for (name, pin) in PINS {
        require(
            admitted.contains(&root.join(name)),
            "mandatory original fixture/checker source not captured",
        )?;
        bounds.push(Bound::open(&root.join(name), Some(pin))?);
    }
    for name in REQUIRED_SOURCE {
        require(
            admitted.contains(&root.join(name)),
            &format!("original dependency not captured: {name}"),
        )?;
    }
    require(
        admitted.contains(&oracle.join("libcar.so"))
            && admitted.contains(&oracle.join("car.cpp"))
            && admitted.contains(&oracle.join("car.h")),
        "current original car model providers not captured",
    )?;
    let wrappers = admitted
        .iter()
        .filter(|p| {
            p.parent() == Some(oracle.as_path())
                && p.file_name().is_some_and(|n| {
                    n.as_encoded_bytes().starts_with(b"ekf_sym_pyx")
                        && n.as_encoded_bytes().ends_with(b".so")
                })
        })
        .count();
    require(
        wrappers == 1,
        "exactly one source-built original wrapper required",
    )?;
    require(
        capture
            .members
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == ["input.json", "results.json", "source.json"],
        "original capture members differ",
    )?;
    let mut original_values = BTreeMap::new();
    for (name, digest) in &capture.members {
        require(hex(digest, 64), "invalid original observation hash")?;
        let bound = Bound::open(&original.join(name), Some(digest))?;
        original_values.insert(name.clone(), numeric::parse_python(&bound.bytes(LIMIT)?)?);
        bounds.push(bound);
    }
    let request = paramsd_fixture_generate::scenarios();
    require(
        request == original_values["input.json"],
        "native estimator request differs from independent original input",
    )?;
    reference::validate_roster(&request["cases"], "operations")?;
    reference::validate_roster(&original_values["source.json"], "rows")?;
    let original_result = &original_values["results.json"];
    require(
        original_result["pass"] == true
            && original_result["cases"] == 7
            && original_result["steps"] == 1196,
        "independent original full comparison did not pass",
    )?;
    // The original loader/oracle lives in the capture's closure. Seal the new
    // controller/decoder itself without borrowing common Cargo/lib registration.
    for name in [
        "rust/crates/dev-tools/src/paramsd_estimator_check.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/identity.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/numeric.rs",
        "rust/crates/dev-tools/src/paramsd_estimator_check/reference.rs",
        "rust/crates/dev-tools/src/bin/check_paramsd_estimator_native.rs",
    ] {
        bounds.push(Bound::open(&root.join(name), None)?);
    }
    let directory = Directory::create(&options.evidence)?;
    let evidence = options.evidence.canonicalize()?;
    check(stop, &directory, &bounds)?;
    let input = evidence.join("input.json");
    let bytes = serde_json::to_vec(&request)?;
    create(&input, &bytes)?;
    bounds.push(Bound::open(&input, None)?);
    let raw = evidence.join("native-raw.json");
    let mut command = CommandSpec::new_os(
        vec![binary.as_os_str().to_owned(), raw.as_os_str().to_owned()],
        Duration::from_secs(180),
    );
    command.input = Some(bytes);
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 1024 * 1024;
    command.cwd = Some(root);
    command.env.insert("PATH".into(), "/nonexistent".into());
    command.env_remove = ["PYTHONPATH", "PYTHONHOME", "LD_PRELOAD"]
        .into_iter()
        .map(Into::into)
        .collect();
    json_file(
        &evidence.join("invocation.json"),
        &json!({"argv":[binary,raw],"binary_sha256":capture.binary.sha256,"source_commit":options.source_commit,"timeout_seconds":180}),
    )?;
    json_file(&evidence.join("bindings-before.json"), &bindings(&bounds))?;
    let captured = runner.run_to_files(
        &command,
        &evidence.join("stdout.log"),
        &evidence.join("stderr.log"),
    );
    let Captured { code, .. } = match captured {
        Ok(value) => value,
        Err(error) => {
            json_file(
                &evidence.join("process.json"),
                &json!({"error":error.to_string(),"cancelled":stop.load(Ordering::SeqCst)}),
            )?;
            return Err(error);
        }
    };
    json_file(&evidence.join("process.json"), &json!({"exit":code}))?;
    check(stop, &directory, &bounds)?;
    require(code == 0, &format!("native estimator child exited {code}"))?;
    let output = Bound::open(&raw, None)?;
    let actual: Value = serde_json::from_slice(&output.bytes(LIMIT)?)?;
    let summary = reference::compare(&original_values["source.json"], &actual)?;
    bounds.push(output);
    check(stop, &directory, &bounds)?;
    let result = serde_json::to_value(&summary)?;
    // Same run source and target: the original checker already observed this ELF.
    // Compare summary as a secondary audit, never as the primary expected trace.
    require(
        result == *original_result,
        "native/original comparison summaries differ",
    )?;
    json_file(&evidence.join("bindings-after.json"), &bindings(&bounds))?;
    writeln!(printed, "{}", serde_json::to_string(&result)?)?;
    printed.flush()?;
    check(stop, &directory, &bounds)?;
    let path = evidence.join("results.json");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let mut receipt = Receipt {
        path,
        file,
        committed: false,
    };
    receipt
        .file
        .write_all(&serde_json::to_vec_pretty(&result)?)?;
    receipt.file.sync_all()?;
    check(stop, &directory, &bounds)?;
    require(receipt.owns(), "estimator result receipt replaced")?;
    File::open(&evidence)?.sync_all()?;
    require(
        !stop.load(Ordering::SeqCst),
        "estimator cancelled during finalization",
    )?;
    receipt.committed = true;
    Ok(result)
}
struct Signals {
    stop: Arc<AtomicBool>,
    ids: Vec<signal_hook::SigId>,
}
impl Signals {
    fn new() -> Result<Self> {
        let mut signals = Self {
            stop: Arc::new(AtomicBool::new(false)),
            ids: Vec::new(),
        };
        for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signals
                .ids
                .push(signal_hook::flag::register(sig, Arc::clone(&signals.stop))?);
        }
        Ok(signals)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
struct Controlled(Arc<AtomicBool>);
impl Runner for Controlled {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(spec, Instant::now() + spec.timeout, &self.0)
    }
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut values = BTreeMap::new();
    while let Some(flag) = args.next() {
        let flag = flag
            .into_string()
            .map_err(|_| "non-UTF-8 estimator option")?;
        require(
            [
                "--root",
                "--binary",
                "--original",
                "--oracle",
                "--evidence",
                "--source-commit",
            ]
            .contains(&flag.as_str()),
            "unknown estimator option",
        )?;
        let value = args.next().ok_or("estimator option value missing")?;
        require(
            values.insert(flag, value).is_none(),
            "duplicate estimator option",
        )?;
    }
    let mut take = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("{name} required"))
    };
    let options = Options {
        root: take("--root")?.into(),
        binary: take("--binary")?.into(),
        original: take("--original")?.into(),
        oracle: take("--oracle")?.into(),
        evidence: take("--evidence")?.into(),
        source_commit: take("--source-commit")?
            .into_string()
            .map_err(|_| "non-UTF-8 source commit")?,
    };
    let signals = Signals::new()?;
    execute(
        &options,
        &mut Controlled(Arc::clone(&signals.stop)),
        &signals.stop,
        &mut std::io::stdout().lock(),
    )?;
    Ok(())
}
