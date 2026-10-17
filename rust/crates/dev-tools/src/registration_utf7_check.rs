//! Fixed original CPython UTF-7 corpus; no candidate decoder produces expectations.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<serde_json::Value> {
    let bytes = include_bytes!("../tests/fixtures/registration-utf7/original.json");
    if hash(bytes) != "ebbee381fabd9375cf0bede9e4aa39bf99d2519d3adb375598090e72918248da" {
        return Err("UTF-7 original corpus changed".into());
    }
    let corpus: serde_json::Value = serde_json::from_slice(bytes)?;
    match fs::read(root.join("rust/tools/check_registration_utf7.py")) {
        Ok(bytes) if corpus["source_sha256"] != hash(&bytes) => {
            return Err("UTF-7 source changed; independently recapture".into())
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    Ok(corpus)
}
fn splitlines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }
    let mut lines = text
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
            '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}
/// Run all107 captured cases, preserve child/JSON/cardinality/write failure order,
/// and permit runner prefixes for the original cross-target invocation contract.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runners: &[OsString],
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let original = corpus(root)?;
    let fixed = &original["fixed"];
    let mut argv = runners.to_vec();
    argv.push(resolve_non_strict(binary)?.into_os_string());
    let mut command = CommandSpec::new_os(argv.clone(), Duration::MAX);
    command.input = Some(
        fixed["input"]
            .as_str()
            .ok_or("UTF-7 input missing")?
            .as_bytes()
            .to_vec(),
    );
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let result = runner.run(&command)?;
    // subprocess text=True decodes both pipes even if check=True then rejects exit.
    let stdout = String::from_utf8(result.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let _stderr = String::from_utf8(result.stderr)?;
    if result.code != 0 {
        return Err(format!("UTF-7 candidate exited {}", result.code).into());
    }
    let actual = splitlines(&stdout)
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    let expected = fixed["expected"]
        .as_array()
        .ok_or("UTF-7 expected missing")?;
    if actual.len() != expected.len() {
        return Err("UTF-7 output cardinality differs".into());
    }
    let mut cases = vec![];
    let mut failures = vec![];
    for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
        let source = JsonValue::parse(&serde_json::to_string(a)?)?;
        let row = format!(
            "{{\"hex\": {}, \"source\": {}, \"native\": {}}}",
            fixed["hex"][index],
            source.to_json()?,
            b.to_json()?
        );
        if !json::equal(&source, b) {
            failures.push(row.clone());
        }
        cases.push(row);
    }
    let args = argv
        .iter()
        .map(|s| python_fs::json_value(s).to_json())
        .collect::<std::result::Result<Vec<_>, _>>()?
        .join(", ");
    let report=JsonValue::parse(&format!("{{\"argv\": [{args}], \"binary_sha256\": \"{}\", \"exit\": {}, \"cases\": [{}], \"failures\": [{}]}}",hash(&fs::read(binary)?),result.code,cases.join(", "),failures.join(", ")))?;
    fs::write(output, json::pretty_with_limit(&report, 0, 995)?)?;
    if !failures.is_empty() {
        return Err("original/native UTF-7 cases differ".into());
    }
    writeln!(
        printed,
        "{} UTF-7 valid/malformed/padding/truncation/direct-byte/surrogate cases PASS",
        expected.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os_with_positionals(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[OptionSpec::append("--runner")],
        2,
    )?;
    if args.help {
        println!("usage: check-registration-utf7 [--runner RUNNER] BINARY OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&args.positionals[0]),
        Path::new(&args.positionals[1]),
        args.append_values
            .get("--runner")
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
