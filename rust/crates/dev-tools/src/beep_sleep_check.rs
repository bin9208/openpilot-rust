//! Independent fixed CPython timeout conversion gate for the beep fixture.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_beepd_sleep.py";
const CHECKER_SHA: &str = "1c287b47f4a1777671db752c431ad991b21ab3e9820214db84cf970a09aa36a7";
#[derive(Deserialize)]
struct Corpus {
    input: String,
    bits: Vec<u64>,
    expected: Vec<String>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<Corpus> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/beep-sleep/manifest.json"))?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.12."))
    {
        return Err("beep sleep original source/CPython provenance changed".into());
    }
    match fs::read(root.join(CHECKER)) {
        Ok(bytes) if sha(&bytes) != CHECKER_SHA => {
            return Err("reintroduced beep sleep checker changed".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let packed = include_bytes!("../tests/fixtures/beep-sleep/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("beep sleep compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing beep sleep corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("beep sleep corpus changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.bits.len() != 27 || corpus.expected.len() != 27 || manifest["cases"] != 27 {
        return Err("beep sleep corpus count changed".into());
    }
    Ok(corpus)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
// Python's `key in value` tests mapping keys, list element equality or string
// substrings. Scalar JSON values fail here even when the source is an error.
fn contains(value: &JsonValue, key: &str) -> Result<bool> {
    let needle = JsonValue::text(key);
    Ok(match value.view() {
        JsonView::Object(_) => value.get(key).is_some(),
        JsonView::Array(values) => values.iter().any(|v| json::equal(v, &needle)),
        JsonView::Text(points) => {
            let needle = key.chars().map(u32::from).collect::<Vec<_>>();
            points.windows(needle.len()).any(|v| v == needle)
        }
        _ => return Err("beep sleep candidate row does not support membership".into()),
    })
}
/// Preserve the original fixed sleep-conversion comparison, including its
/// error-key-only acceptance and strict zip before any artifact is written.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let corpus = corpus(root)?;
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
        return Err(format!("beep sleep candidate exited {}", child.code).into());
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
    if actual.len() != corpus.bits.len() {
        return Err("beep sleep candidate output count differs".into());
    }
    let mut rows = vec![];
    let mut failures = vec![];
    for ((bits, expected), native) in corpus.bits.iter().zip(&corpus.expected).zip(actual) {
        let expected = json::parse_text(expected)?;
        let failed = contains(&expected, "error")? != contains(&native, "error")?
            || (contains(&expected, "nanoseconds")? && !json::equal(&expected, &native));
        let row = format!(
            "{{\"seconds_bits\":{bits},\"source\":{},\"native\":{}}}",
            expected.to_json()?,
            native.to_json()?
        );
        if failed {
            failures.push(row.clone());
        }
        rows.push(row);
    }
    let passed = failures.is_empty();
    let report = format!(
        "{{\"passed\":{passed},\"cases\":{},\"failures\":[{}],\"results\":[{}]}}",
        rows.len(),
        failures.join(","),
        rows.join(",")
    );
    fs::write(output.join("native.stdout"), stdout)?;
    // json.dumps evaluates before write_text opens summary.json. The standalone
    // original CLI accepts 991 nested candidate error arrays, then fails here
    // while preserving an existing summary and the newly written native.stdout.
    let summary = json::pretty_with_limit(&json::parse_text(&report)?, 0, 995)? + "\n";
    fs::write(output.join("summary.json"), summary)?;
    writeln!(
        printed,
        "{{\"passed\": {passed}, \"cases\": {}, \"failures\": {}}}",
        rows.len(),
        failures.len()
    )?;
    if !passed {
        return Err("beep sleep timeout conversions differ".into());
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
        println!("usage: check-beep-sleep --binary EXAMPLE --output DIRECTORY");
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
