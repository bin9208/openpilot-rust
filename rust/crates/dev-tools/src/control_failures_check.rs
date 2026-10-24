//! Native replacement of the27-case rejection controller, with independent
//! saved original exceptions and exact full-schema fixture comparison.
mod evidence;
pub mod fixtures;
mod inputs;
use crate::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use evidence::{Case, Evidence};
use inputs::Inputs;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct Options {
    pub root: PathBuf,
    pub trace: PathBuf,
    pub numerics: PathBuf,
    pub evidence: PathBuf,
    pub reference: PathBuf,
    pub reference_sha256: String,
}
fn check(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Relaxed) {
        Err("control rejection check cancelled".into())
    } else {
        Ok(())
    }
}
fn bounded_json(path: &Path) -> Result<Value> {
    if !fs::symlink_metadata(path)?.is_file() || fs::metadata(path)?.len() > 32 << 20 {
        return Err("reference/output must be regular and at most32MiB".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn source_tree(held: &mut Inputs, root: &Path) -> Result<()> {
    for name in [
        "rust/crates/controlsd/src",
        "rust/crates/control-policy/src",
        "rust/crates/dev-tools/src/controlsd_fixture_generate",
        "rust/crates/dev-tools/src/control_failures_check",
    ] {
        held.tree(&root.join(name))?;
    }
    for name in [
        "rust/crates/controlsd/examples/controlsd_trace.rs",
        "rust/crates/dev-tools/src/controlsd_fixture_generate.rs",
        "rust/crates/dev-tools/src/control_failures_check.rs",
        "rust/crates/dev-tools/src/qa_process.rs",
        "rust/crates/dev-tools/src/bin/check_control_failures_native.rs",
        "rust/crates/dev-tools/src/lib.rs",
        "rust/crates/dev-tools/Cargo.toml",
        "rust/crates/controlsd/Cargo.toml",
        "rust/crates/control-policy/Cargo.toml",
        "rust/Cargo.toml",
        "rust/Cargo.lock",
        "rust/rust-toolchain.toml",
    ] {
        held.add(&root.join(name))?;
    }
    Ok(())
}
fn run_case<'a>(
    case: &Value,
    options: &Options,
    assets: &Path,
    owned: &'a Evidence,
    runner: &mut dyn Runner,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(Captured, Case<'a>)> {
    check(stop)?;
    owned.verify()?;
    let proof = owned.case(case["name"].as_str().ok_or("case name missing")?)?;
    let directory = proof.anchor();
    let output = directory.join("native.json");
    let request = serde_json::to_vec(&json!({"cases":[case]}))?;
    Evidence::bytes(&directory.join("input.json"), &request)?;
    let mut spec = CommandSpec::new_os(
        vec![
            options.trace.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ],
        Duration::from_secs(30),
    );
    spec.cwd = Some(options.root.clone());
    spec.input = Some(request);
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    spec.env.insert(
        "CONTROLS_NUMERICS".into(),
        options
            .numerics
            .to_str()
            .ok_or("numerics path must be UTF-8, matching source environment")?
            .into(),
    );
    spec.env.insert(
        "CONTROLS_ASSETS".into(),
        assets.to_str().ok_or("assets path must be UTF-8")?.into(),
    );
    Evidence::json(
        &directory.join("invocation.json"),
        &json!({"argv":spec.child_argv().iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"cwd":options.root,"environment_overlay":spec.env,"timeout_seconds":30}),
    )?;
    if Instant::now() >= deadline {
        return Err("aggregate rejection deadline180s exceeded".into());
    }
    spec.timeout = spec
        .timeout
        .min(deadline.saturating_duration_since(Instant::now()));
    // Inherit only the held CASE descriptor. Every child output and receipt
    // stays on that inode even when ROOT/CASE is replaced by foreign data.
    let inherit = proof.inherit()?;
    let captured = runner.run(&spec);
    drop(inherit);
    let captured = match captured {
        Ok(value) => value,
        Err(error) => {
            if let Some(crate::qa_process::CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<crate::qa_process::CaptureError>()
            {
                Evidence::bytes(&directory.join("stdout.bin"), stdout)?;
                Evidence::bytes(&directory.join("stderr.bin"), stderr)?;
            }
            Evidence::json(
                &directory.join("capture-error.json"),
                &json!({"error":error.to_string(),"accepted":false}),
            )?;
            proof.verify()?;
            return Err(error);
        }
    };
    Evidence::bytes(&directory.join("stdout.bin"), &captured.stdout)?;
    Evidence::bytes(&directory.join("stderr.bin"), &captured.stderr)?;
    Evidence::json(
        &directory.join("status.json"),
        &json!({"code":captured.code}),
    )?;
    check(stop)?;
    proof.verify()?;
    Ok((captured, proof))
}
/// Single-controller operation. The caller must not concurrently spawn unrelated
/// children while run_case temporarily inherits its evidence directory FD.
pub fn run(options: &Options, runner: &mut dyn Runner, stop: &AtomicBool) -> Result<Value> {
    check(stop)?;
    let owned = Evidence::claim(&options.evidence)?; // Reject every occupied leaf before children.
    let root = options.root.canonicalize()?;
    let trace = options.trace.canonicalize()?;
    let numerics = options.numerics.canonicalize()?;
    let reference = options.reference.canonicalize()?;
    let options = Options {
        root,
        trace,
        numerics,
        evidence: options.evidence.clone(),
        reference,
        reference_sha256: options.reference_sha256.clone(),
    };
    let mut held = Inputs::default();
    held.add(&options.reference)?;
    held.add(&options.trace)?;
    held.tree(&options.numerics)?;
    let assets = std::env::var_os("CONTROLS_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| options.root.join("opendbc_repo/opendbc/car/torque_data"));
    let assets = if assets.is_absolute() {
        assets
    } else {
        options.root.join(assets)
    };
    let assets = assets.canonicalize()?;
    held.tree(&assets)?;
    let sources = fixtures::source_hashes(&options.root)?;
    for name in sources.keys() {
        held.add(&options.root.join(name))?;
    }
    source_tree(&mut held, &options.root)?;
    if crate::seal::digest(&options.reference)? != options.reference_sha256 {
        return Err("original reference SHA256 differs".into());
    }
    let reference = bounded_json(&options.reference)?;
    if reference["format"] != 1 || reference["sources"] != serde_json::to_value(&sources)? {
        return Err("independent original source contract changed".into());
    }
    let cases = fixtures::cases()?;
    let health = fixtures::health()?;
    Evidence::json(
        &owned.anchor().join("fixtures.native.json"),
        &json!({"health":health,"cases":cases}),
    )?;
    let projections = cases
        .iter()
        .map(fixtures::projection)
        .collect::<Result<Vec<_>>>()?;
    Evidence::json(
        &owned.anchor().join("fixtures.native-projection.json"),
        &json!({"health":fixtures::projection(&health)?,"cases":projections}),
    )?;
    let expected = reference["cases"]
        .as_array()
        .ok_or("original rejection records missing")?;
    if expected.len() != cases.len() || reference["health"]["source_frames"] != 5 {
        return Err("original rejection/health roster differs".into());
    }
    for ((case, original), projection) in cases.iter().zip(expected).zip(&projections) {
        if original["case"]["name"] != case["name"]
            || original["projection"] != fixtures::projection(&original["case"])?
            || original["projection"] != *projection
        {
            return Err(format!(
                "exact independent fixture projection differs: {}",
                case["name"]
            )
            .into());
        }
        if original["source"]["exception"]
            .as_str()
            .filter(|s| !s.is_empty())
            .is_none()
            || original["source"]["message"].as_str().is_none()
        {
            return Err("original exception record missing".into());
        }
    }
    if reference["health"]["projection"] != fixtures::projection(&reference["health"]["case"])?
        || reference["health"]["projection"] != fixtures::projection(&health)?
    {
        return Err("exact independent health fixture differs".into());
    }
    held.verify()?;
    let before = held.report();
    Evidence::json(&owned.anchor().join("inputs.before.json"), &before)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let result = (|| -> Result<Value> {
        let (status, health_proof) =
            run_case(&health, &options, &assets, &owned, runner, stop, deadline)?;
        if status.code != 0 {
            return Err(
                "healthy trace failed; rejection cannot establish malformed-input rejection".into(),
            );
        }
        let output = bounded_json(&health_proof.anchor().join("native.json"))?;
        health_proof.verify()?;
        let rows = output
            .as_array()
            .filter(|v| v.len() == 1)
            .ok_or("health output case roster differs")?;
        if rows[0]["name"] != health["name"] || rows[0]["rows"].as_array().map(Vec::len) != Some(5)
        {
            return Err("health output frames differ".into());
        }
        let mut results = Vec::new();
        for (case, original) in cases.iter().zip(expected) {
            let (status, case_proof) =
                run_case(case, &options, &assets, &owned, runner, stop, deadline)?;
            let path = case_proof.anchor().join("native.json");
            // lstat catches regular, special and dangling/ordinary alias outputs.
            let absent = match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                Ok(_) => false,
                Err(e) => return Err(e.into()),
            };
            if status.code == 0 || !absent {
                return Err(
                    format!("native did not reject without output: {}", case["name"]).into(),
                );
            }
            Evidence::json(
                &case_proof.anchor().join("source.json"),
                &original["source"],
            )?;
            case_proof.verify()?;
            results.push(json!({"name":case["name"],"source":original["source"]["exception"],"native_exit":status.code,"pass":true}));
        }
        Ok(
            json!({"pass":true,"cases":results,"reference_sha256":options.reference_sha256,"original_exception_origin":"independent unchanged controlsd_source.trace","fixture_projection":"complete schema and exact float bits","health_frames":5,"sources_retired":false,"runtime_numeric_acceptance":false}),
        )
    })();
    // Verify even after a failed/cancelled child; a teardown failure cannot be
    // turned into success by a preceding nonzero trace exit.
    let verification = held.verify().and_then(|()| owned.verify());
    let after = held.report();
    Evidence::json(&owned.anchor().join("inputs.after.json"), &after)?;
    verification?;
    if before != after {
        return Err("physical held input snapshots differ".into());
    }
    held.verify()?;
    owned.verify()?;
    check(stop)?;
    let report = result?;
    owned.report(&report)?;
    Ok(report)
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
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
    let mut values = BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" && values.is_empty() {
            println!("check-control-failures-native --root ROOT --trace TRACE --numerics DIR --reference JSON --reference-sha256 SHA --evidence NEW_DIR");
            return Ok(());
        }
        let flag = flag.into_string().map_err(|_| "option must be UTF-8")?;
        if ![
            "--root",
            "--trace",
            "--numerics",
            "--reference",
            "--reference-sha256",
            "--evidence",
        ]
        .contains(&flag.as_str())
        {
            return Err("unknown option".into());
        }
        let value = args.next().ok_or("option value missing")?;
        if values.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let mut take = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("{name} required"))
    };
    let options = Options {
        root: take("--root")?.into(),
        trace: take("--trace")?.into(),
        numerics: take("--numerics")?.into(),
        reference: take("--reference")?.into(),
        reference_sha256: take("--reference-sha256")?
            .into_string()
            .map_err(|_| "SHA256 must be UTF-8")?,
        evidence: take("--evidence")?.into(),
    };
    if options.reference_sha256.len() != 64
        || !options
            .reference_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("reference SHA256 must be64lowercase hex characters".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let report = run(&options, &mut Controlled(stop.clone()), &stop)?;
    check(&stop)?;
    let mut printed = std::io::stdout().lock();
    writeln!(
        printed,
        "{}",
        json!({"pass":true,"cases":report["cases"].as_array().map(Vec::len)})
    )?;
    printed.flush()?;
    Ok(())
}
