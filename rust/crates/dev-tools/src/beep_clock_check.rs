//! Independent fixed CPython monotonic conversion gate for the beep fixture.
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
const CHECKER: &str = "rust/tools/check_beepd_clock.py";
const CHECKER_SHA: &str = "fa6b799820780f1306b668de617c7d505a42bf074088d66a5d6e0821685d0b1e";
#[derive(Deserialize)]
struct Corpus {
    input: String,
    inputs_json: String,
    values: Vec<i64>,
    expected: Vec<String>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<Corpus> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/beep-clock/manifest.json"))?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|s| s.starts_with("3.12."))
    {
        return Err("beep clock original source/CPython provenance changed".into());
    }
    match fs::read(root.join(CHECKER)) {
        Ok(bytes) if sha(&bytes) != CHECKER_SHA => {
            return Err("reintroduced beep clock checker changed".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let packed = include_bytes!("../tests/fixtures/beep-clock/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("beep clock compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing beep clock corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("beep clock corpus changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.values.len() != 1011 || corpus.expected.len() != 1011 || manifest["cases"] != 1011 {
        return Err("beep clock corpus count changed".into());
    }
    Ok(corpus)
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
    let corpus = corpus(root)?;
    // Preserve mkdir(parents=True, exist_ok=True), before binary.resolve and
    // child invocation. Existing receipts are overwritten only at source stages.
    fs::create_dir_all(output)?;
    let binary = resolve_non_strict(binary)?;
    let mut spec = CommandSpec::new_os(vec![binary.into_os_string()], Duration::MAX);
    spec.input = Some(corpus.input.into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let child = runner.run(&spec)?;
    let stdout = text(child.stdout)?;
    text(child.stderr)?;
    if child.code != 0 {
        return Err(format!("beep clock candidate exited {}", child.code).into());
    }
    let actual = crate::stats_clock_check::integers(&stdout)?;
    fs::write(output.join("native.stdout"), stdout)?;
    fs::write(output.join("inputs.json"), corpus.inputs_json)?;
    // Python computes the strict zip only after these two artifact writes.
    if actual.len() != corpus.values.len() {
        return Err("beep clock output count differs".into());
    }
    let mut differences = vec![];
    for ((value, expected), native) in corpus.values.iter().zip(&corpus.expected).zip(actual) {
        if *expected != native {
            differences.push(format!(
                "{{\"nanoseconds\":{value},\"python_bits\":{expected},\"native_bits\":{native}}}"
            ));
        }
    }
    let passed = differences.is_empty();
    let report = json::parse_text(&format!(
        "{{\"passed\":{passed},\"cases\":{},\"differences\":[{}]}}",
        corpus.values.len(),
        differences.join(",")
    ))?;
    fs::write(
        output.join("summary.json"),
        json::pretty(&report, 0)? + "\n",
    )?;
    writeln!(
        printed,
        "{{\"passed\": {passed}, \"cases\": {}, \"differences\": {}}}",
        corpus.values.len(),
        differences.len()
    )?;
    if !passed {
        return Err("beep clock monotonic conversions differ".into());
    }
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
        println!("usage: check-beep-clock --binary EXAMPLE --output DIRECTORY");
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
