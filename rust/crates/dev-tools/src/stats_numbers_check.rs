//! Independently captured fixed statsd numeric aggregation corpus.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_stats_numbers.py";
const CHECKER_SHA: &str = "b4e2e2b88c629f289333ebd3bb5efa9893a4a9037dd26a2cdad177ece3db303c";
#[derive(Deserialize)]
struct Case {
    input: String,
    expected: String,
}
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn corpus(root: &Path) -> Result<(Vec<Case>, String)> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/stats-numbers/manifest.json"
    ))?;
    let source = "openpilot/system/statsd.py";
    let source_sha = sha(&fs::read(root.join(source))?);
    if manifest["sources"][source] != source_sha || manifest["sources"][CHECKER] != CHECKER_SHA {
        return Err("stats numeric source provenance changed; independently recapture".into());
    }
    match fs::read(root.join(CHECKER)) {
        Ok(bytes) if sha(&bytes) != CHECKER_SHA => {
            return Err("reintroduced stats numeric checker changed".into())
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let packed = include_bytes!("../tests/fixtures/stats-numbers/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("stats numeric compressed corpus changed".into());
    }
    let size = manifest["bytes"].as_u64().ok_or("missing corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("stats numeric corpus changed".into());
    }
    let rows: Vec<Case> = serde_json::from_slice(&plain)?;
    if manifest["cases"].as_u64() != Some(rows.len() as u64) {
        return Err("stats numeric corpus count changed".into());
    }
    Ok((rows, source_sha))
}
// pathlib removes repeated separators and dot components, without resolving
// symlinks or '..'. In particular './name' becomes a PATH-searched executable.
fn lexical(path: &Path) -> PathBuf {
    let b = path.as_os_str().as_bytes();
    let mut out = if b.starts_with(b"//") && !b.starts_with(b"///") {
        b"//".to_vec()
    } else if b.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for p in b
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend(p);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    OsString::from_vec(out).into()
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: Option<&Path>,
    process: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let (rows, source_sha) = corpus(root)?;
    let binary = lexical(binary);
    let output = lexical(output);
    let mut argv = vec![];
    if let Some(runner) = runner {
        argv.push(lexical(runner).into_os_string());
    }
    argv.push(binary.clone().into_os_string());
    let mut spec = CommandSpec::new_os(argv, Duration::MAX);
    spec.input = Some(
        rows.iter()
            .map(|row| format!("{}\n", row.input))
            .collect::<String>()
            .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = process.run(&spec)?;
    let stdout = String::from_utf8(captured.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    String::from_utf8(captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("stats candidate exited {}", captured.code).into());
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
    if actual.len() != rows.len() {
        return Err("stats candidate output count differs".into());
    }
    let mut differences = vec![];
    for (index, (row, actual)) in rows.iter().zip(actual).enumerate() {
        let expected = JsonValue::text(&row.expected);
        if !json::equal(&expected, &actual) {
            differences.push(format!(
                "{{\"case\":{index},\"expected\":{},\"actual\":{},\"input\":{}}}",
                expected.to_json()?,
                actual.to_json()?,
                row.input
            ));
        }
    }
    let report=json::parse_text(&format!("{{\"cases\":{},\"differences\":[{}],\"source_sha256\":\"{source_sha}\",\"binary_sha256\":\"{}\"}}",rows.len(),differences.join(","),sha(&fs::read(binary)?)))?;
    fs::write(output, json::pretty_with_limit(&report, 0, 996)?)?;
    writeln!(
        printed,
        "{} aggregation cases, {} differences",
        rows.len(),
        differences.len()
    )?;
    if !differences.is_empty() {
        return Err("stats numeric aggregation differs".into());
    }
    Ok(())
}
pub fn cli() -> Result<()> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--runner", false),
        ],
    )?;
    if options.help {
        println!("usage: check-stats-numbers --binary BINARY --output PATH [--runner RUNNER]");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        options.values.get("--runner").map(Path::new),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
