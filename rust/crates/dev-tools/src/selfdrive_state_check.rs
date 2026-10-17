//! Fixed original selfdrived StateMachine differential; generic runtime Python remains live.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_selfdrive_state.py";
const CHECKER_SHA: &str = "64cae01e9432f791b7e5455b0125e3833d61c965a1e4569d0af5a37a4cd12af9";
const LIVE_SOURCES: [&str; 3] = [
    "openpilot/selfdrive/selfdrived/state.py",
    "openpilot/selfdrive/selfdrived/events.py",
    "openpilot/common/realtime.py",
];
#[derive(Deserialize)]
struct Corpus {
    input: String,
    source: String,
    native: String,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<(Corpus, serde_json::Value)> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/selfdrive-state/manifest.json"
    ))?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["steps"] != 51023
        || manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.12."))
    {
        return Err("selfdrive state source/interpreter provenance changed".into());
    }
    let sources = manifest["sources"]
        .as_object()
        .ok_or("missing selfdrive state source pins")?;
    if sources.len() != 4 || LIVE_SOURCES.iter().any(|name| !sources.contains_key(*name)) {
        return Err("selfdrive state live source pin set changed".into());
    }
    for (name, pin) in sources {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(format!("{name} changed; independently recapture selfdrive state").into());
        }
    }
    let packed = include_bytes!("../tests/fixtures/selfdrive-state/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("selfdrive state compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing selfdrive state corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("selfdrive state corpus changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.input.lines().count() != 51023 || corpus.native.lines().count() != 51023 {
        return Err("selfdrive state corpus count changed".into());
    }
    Ok((corpus, manifest))
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let (corpus, manifest) = corpus(root)?;
    // Original exist_ok=False: existing evidence must never be overwritten.
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    fs::write(output.join("input.jsonl"), &corpus.input)?;
    fs::write(output.join("source.json"), &corpus.source)?;
    let mut spec = CommandSpec::new_os(
        vec![resolve_non_strict(binary)?.into_os_string()],
        Duration::from_secs(30),
    );
    spec.input = Some(corpus.input.into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let child = runner.run(&spec)?;
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join("native.stdout"), &stdout)?;
    fs::write(output.join("native.stderr"), &stderr)?;
    if child.code != 0 {
        return Err(format!("selfdrive state candidate exited {}", child.code).into());
    }
    let mut lines = stdout
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let actual = lines
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    if actual.len() != 51023 {
        return Err("selfdrive state candidate row count differs".into());
    }
    for (expected, actual) in corpus.native.lines().zip(actual) {
        if !json::equal(&json::parse_text(expected)?, &actual) {
            return Err("selfdrive state source/native state machine response differs".into());
        }
    }
    // The original hashes the supplied binary pathname after comparison, not
    // its resolved child path, and publishes no manifest if that read fails.
    let binary_sha = sha(&fs::read(binary)?);
    let report = json::parse_text(&format!(
        "{{\"pass\":true,\"steps\":51023,\"exhaustive_single_steps\":30720,\"source_sha256\":{},\"binary_sha256\":\"{binary_sha}\"}}",
        manifest["sources"]["openpilot/selfdrive/selfdrived/state.py"]
    ))?;
    fs::write(
        output.join("result.json"),
        json::pretty_with_limit(&report, 0, 995)?,
    )?;
    writeln!(printed, "{}", report.to_json()?)?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-selfdrive-state --binary EXAMPLE --output DIRECTORY");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
