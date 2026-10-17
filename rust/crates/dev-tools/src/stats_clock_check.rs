//! Independent fixed datetime and CPython monotonic clock regression gate.
use crate::{
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_stats_clock.py";
const CHECKER_SHA: &str = "cd45caf29927932c0ddb5cac1b230d7575b31f34880310ee8e5e135c7267ba01";
#[derive(Deserialize)]
struct Calendar {
    input: String,
    expected: String,
}
#[derive(Deserialize)]
struct Monotonic {
    input: String,
    nanos: u64,
    expected: String,
}
#[derive(Deserialize)]
struct Boundary {
    input: String,
    seconds: i64,
    source_error: String,
}
#[derive(Deserialize)]
struct Corpus {
    calendar: Vec<Calendar>,
    monotonic: Vec<Monotonic>,
    boundaries: Vec<Boundary>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<Corpus> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/stats-clock/manifest.json"))?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.12."))
    {
        return Err("stats clock original source/CPython provenance changed".into());
    }
    match fs::read(root.join(CHECKER)) {
        Ok(bytes) if sha(&bytes) != CHECKER_SHA => {
            return Err("reintroduced stats clock checker changed".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let packed = include_bytes!("../tests/fixtures/stats-clock/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("stats clock compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing clock corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("stats clock corpus changed".into());
    }
    let result: Corpus = serde_json::from_slice(&plain)?;
    for (name, actual, required) in [
        ("calendar_cases", result.calendar.len(), 5048),
        ("monotonic_cases", result.monotonic.len(), 5009),
        ("boundary_cases", result.boundaries.len(), 2),
    ] {
        if actual != required || manifest[name].as_u64() != Some(actual as u64) {
            return Err("stats clock corpus count changed".into());
        }
    }
    Ok(result)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn decoded(captured: Captured, checked: bool) -> Result<(i32, String, String)> {
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    if checked && captured.code != 0 {
        return Err(format!("stats clock candidate exited {}", captured.code).into());
    }
    Ok((captured.code, stdout, stderr))
}
fn integers(text: &str) -> Result<Vec<String>> {
    let mut lines = text
        .split([
            '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| {
            // Reuse the native Python int grammar, arbitrary precision and digit
            // limit. Parsing as i128/u64 would reject source-admitted differences.
            Ok(openpilot_carrot_navi::json::Value::text(line)
                .int()?
                .to_string())
        })
        .collect()
}
fn command(binary: &Path, input: String, monotonic: bool) -> CommandSpec {
    let mut argv = vec![binary.as_os_str().to_owned()];
    if monotonic {
        argv.push("--monotonic".into());
    }
    let mut command = CommandSpec::new_os(argv, Duration::MAX);
    command.input = Some(input.into_bytes());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    command
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let binary = resolve_non_strict(binary)?;
    let output = resolve_non_strict(output)?;
    let corpus = corpus(root)?;
    let calendar = command(
        &binary,
        corpus
            .calendar
            .iter()
            .map(|r| format!("{}\n", r.input))
            .collect(),
        false,
    );
    let (_, stdout, _) = decoded(runner.run(&calendar)?, true)?;
    let actual = integers(&stdout)?;
    if actual.len() != corpus.calendar.len() {
        return Err("stats calendar output count differs".into());
    }
    let mut differences = vec![];
    for (row, native) in corpus.calendar.iter().zip(actual) {
        if row.expected != native {
            differences.push(format!(
                "{{\"input\":{},\"source\":{},\"native\":{native}}}",
                row.input, row.expected
            ));
        }
    }
    let monotonic = command(
        &binary,
        corpus
            .monotonic
            .iter()
            .map(|r| format!("{}\n", r.input))
            .collect(),
        true,
    );
    let (_, stdout, _) = decoded(runner.run(&monotonic)?, true)?;
    let actual = integers(&stdout)?;
    if actual.len() != corpus.monotonic.len() {
        return Err("stats monotonic output count differs".into());
    }
    let mut mono_differences = vec![];
    for (row, native) in corpus.monotonic.iter().zip(actual) {
        if row.expected != native {
            mono_differences.push(format!(
                "{{\"input\":{},\"source_bits\":{},\"native_bits\":{native}}}",
                row.nanos, row.expected
            ));
        }
    }
    let mut boundaries = vec![];
    for boundary in &corpus.boundaries {
        let spec = command(&binary, format!("{}\n", boundary.input), false);
        let (code, _, stderr) = decoded(runner.run(&spec)?, false)?;
        if code == 0 {
            return Err("stats candidate accepted unsupported calendar boundary".into());
        }
        boundaries.push(format!(
            "{{\"seconds\":{},\"source_error\":{},\"native_exit\":{code},\"native_stderr\":{}}}",
            boundary.seconds,
            JsonValue::text(&boundary.source_error).to_json()?,
            JsonValue::text(&stderr).to_json()?
        ));
    }
    let report = json::parse_text(&format!("{{\"calendar_errors\":[{}],\"cases\":{},\"differences\":[{}],\"monotonic_cases\":{},\"monotonic_differences\":[{}],\"binary_sha256\":\"{}\"}}", boundaries.join(","), corpus.calendar.len(), differences.join(","), corpus.monotonic.len(), mono_differences.join(","), sha(&fs::read(&binary)?)))?;
    fs::write(output, json::pretty(&report, 0)?)?;
    writeln!(
        printed,
        "{} datetime timestamp cases, {} differences",
        corpus.calendar.len(),
        differences.len()
    )?;
    writeln!(
        printed,
        "{} CPython monotonic conversion cases, {} differences",
        corpus.monotonic.len(),
        mono_differences.len()
    )?;
    if !differences.is_empty() || !mono_differences.is_empty() {
        return Err("stats clock conversions differ".into());
    }
    Ok(())
}
pub fn cli() -> Result<()> {
    // The original uses sys.argv unpacking, so --help is a path and wrong
    // arity exits 1. Resolve every supplied path before unpacking as it does.
    let paths = std::env::args_os()
        .skip(1)
        .map(|v| resolve_non_strict(Path::new(&v)))
        .collect::<Result<Vec<_>>>()?;
    let [binary, output] = paths.as_slice() else {
        return Err("stats clock checker requires EXAMPLE OUTPUT".into());
    };
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        binary,
        output,
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
