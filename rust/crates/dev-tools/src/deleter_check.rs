//! Native deleter controllers using immutable, independently captured originals.
mod corpus;
mod daemon;
mod owned;
mod process;
mod reference;
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    root: PathBuf,
    fixture: PathBuf,
    fixture_sha256: String,
    trace: PathBuf,
    binary: PathBuf,
    output: PathBuf,
    scope: String,
    provider_pins: BTreeMap<PathBuf, String>,
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn save(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
fn providers(options: &Options) -> Result<()> {
    require(
        options.provider_pins.len() == 2
            && options.provider_pins.contains_key(&options.trace)
            && options.provider_pins.contains_key(&options.binary),
        "deleter provider roster differs",
    )?;
    for (path, pin) in &options.provider_pins {
        require(
            corpus::hash(&corpus::bytes(path, 256 << 20)?) == *pin,
            "deleter native provider bytes changed",
        )?;
    }
    Ok(())
}
fn authenticated(options: &Options) -> Result<corpus::Corpus> {
    let corpus = corpus::load(&options.root, &options.fixture, &options.fixture_sha256)?;
    providers(options)?;
    Ok(corpus)
}
fn request(path: &Path) -> Result<Options> {
    Ok(serde_json::from_slice(&corpus::bytes(path, 1 << 20)?)?)
}
fn worker(options: &Options) -> Result<Value> {
    let corpus = authenticated(options)?;
    require(
        ["reference", "daemon", "observer", "all"].contains(&options.scope.as_str()),
        "unknown native deleter scope",
    )?;
    fs::create_dir_all(&options.output)?;
    save(
        &options.output.join("inputs.before.json"),
        &json!({"fixture_sha256":options.fixture_sha256,"sources":corpus.source_pins,"providers":options.provider_pins}),
    )?;
    let mut report = json!({"passed":true,"scope":options.scope,"original_fixture_sha256":options.fixture_sha256});
    if options.scope == "reference" || options.scope == "all" {
        let actual = reference::run(&corpus, &options.trace, &options.output.join("reference"))?;
        if options.scope == "all" {
            report["reference"] = actual;
        } else {
            report = actual;
        }
    }
    if options.scope == "daemon" || options.scope == "all" {
        let actual = daemon::run(options, &corpus, &options.output.join("daemon"))?;
        if options.scope == "all" {
            report["daemon"] = actual;
        } else {
            report = actual;
        }
    }
    if options.scope == "observer" || options.scope == "all" {
        let actual = daemon::observer(&options.output.join("observer"))?;
        if options.scope == "all" {
            report["observer"] = actual;
        } else {
            report = actual;
        }
    }
    authenticated(options)?;
    let inputs: Value =
        serde_json::from_slice(&fs::read(options.output.join("inputs.before.json"))?)?;
    save(&options.output.join("inputs.after.json"), &inputs)?;
    report["original_fixture_sha256"] = json!(options.fixture_sha256);
    report["inputs_unchanged"] = json!(true);
    save(&options.output.join("report.json"), &report)?;
    Ok(report)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("--deleter-observer-fixture") => {
            require(args.len() == 2, "deleter observer fixture arguments")?;
            return daemon::emit_observer_fixture(Path::new(&args[1]));
        }
        Some("--deleter-worker") | Some("--deleter-mount") => {
            require(args.len() == 2, "deleter internal worker arguments")?;
            let options = request(Path::new(&args[1]))?;
            let value = if args[0] == "--deleter-mount" {
                let corpus = authenticated(&options)?;
                let value = daemon::mount(&options, &corpus, &options.output)?;
                authenticated(&options)?;
                value
            } else {
                worker(&options)?
            };
            println!("{value}");
            return Ok(());
        }
        _ => {}
    }
    use crate::argparse_scan::{scan, OptionSpec as S};
    let parsed = scan(
        &args,
        &[
            S::value("--root", true),
            S::value("--scope", true),
            S::value("--fixture", true),
            S::value("--fixture-sha256", true),
            S::value("--trace", true),
            S::value("--binary", true),
            S::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!(
            "Usage: check-deleter --root ROOT --scope all|reference|daemon|observer --fixture JSON --fixture-sha256 SHA256 --trace PATH --binary PATH --output DIR"
        );
        return Ok(());
    }
    require(
        parsed.positionals.is_empty(),
        "unexpected deleter controller positional argument",
    )?;
    let get = |key: &str| -> Result<&str> {
        parsed
            .values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("{key} required").into())
    };
    let root = Path::new(get("--root")?).canonicalize()?;
    let fixture = Path::new(get("--fixture")?).canonicalize()?;
    // Reject the oracle before resolving, hashing or starting native providers.
    corpus::load(&root, &fixture, get("--fixture-sha256")?)?;
    let trace = Path::new(get("--trace")?).canonicalize()?;
    let binary = Path::new(get("--binary")?).canonicalize()?;
    require(
        trace != binary,
        "deleter trace and daemon must be distinct providers",
    )?;
    let mut provider_pins = BTreeMap::new();
    for path in [&trace, &binary] {
        provider_pins.insert(path.clone(), corpus::hash(&corpus::bytes(path, 256 << 20)?));
    }
    let output = PathBuf::from(get("--output")?);
    fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let options = Options {
        root,
        fixture,
        fixture_sha256: get("--fixture-sha256")?.to_owned(),
        trace,
        binary,
        output,
        scope: get("--scope")?.to_owned(),
        provider_pins,
    };
    authenticated(&options)?;
    let private = owned::Directory::temporary()?;
    let request = private.path.join("request.json");
    save(&request, &serde_json::to_value(&options)?)?;
    let spec = CommandSpec::new_os(
        vec![
            std::env::current_exe()?.into_os_string(),
            "--deleter-worker".into(),
            request.into_os_string(),
        ],
        Duration::from_secs(90),
    );
    let captured = NativeRunner.run(&spec)?;
    fs::write(options.output.join("worker.stdout.bin"), &captured.stdout)?;
    fs::write(options.output.join("worker.stderr.bin"), &captured.stderr)?;
    require(
        captured.code == 0,
        &format!(
            "native deleter worker failed: {}",
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    let report: Value = serde_json::from_slice(&captured.stdout)?;
    require(
        report["passed"] == true && report["inputs_unchanged"] == true,
        "deleter worker success receipt absent",
    )?;
    authenticated(&options)?;
    println!("{report}");
    Ok(())
}
