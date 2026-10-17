//! Shared process/receipt primitives for the original exposure and misc checkers.
use crate::{
    argparse_scan::{scan_os, OptionSpec, ParsedOs},
    camera_sensor_check::compare_with_limit,
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::Path,
    time::Duration,
};

pub(crate) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn float(value: f64) -> Result<String> {
    Ok(JsonValue::parse(&format!("{value:.17e}"))?.to_json()?)
}
pub(crate) fn string(value: &str) -> Result<String> {
    Ok(JsonValue::text(value).to_json()?)
}
pub(crate) fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
pub(crate) fn splitlines(text: &str) -> Vec<&str> {
    let mut rows = vec![];
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\x0b' | '\x0c' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            rows.push(&text[start..at]);
            start = at + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, v)| *v == '\n') {
                let (at, _) = chars.next().unwrap();
                start = at + 1;
            }
        }
    }
    if start < text.len() {
        rows.push(&text[start..]);
    }
    rows
}
pub(crate) fn execute(runner: &mut dyn Runner, argv: &[OsString], input: &[u8]) -> Result<String> {
    let mut spec = CommandSpec::new_os(argv.to_vec(), Duration::MAX);
    spec.input = Some(input.to_vec());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let result = runner.run(&spec)?;
    let stdout = String::from_utf8(result.stdout)?;
    let _ = String::from_utf8(result.stderr)?;
    if result.code != 0 {
        return Err(format!("camera process exited {}", result.code).into());
    }
    Ok(stdout)
}
pub(crate) fn write_report(
    output: &Path,
    report: &JsonValue,
    printed: &mut dyn Write,
) -> Result<()> {
    let text = json::pretty_with_limit(report, 0, 995)? + "\n";
    fs::write(output.join("report.json"), text)?;
    writeln!(printed, "{}", report.to_json()?)?;
    Ok(())
}
pub(crate) fn compare_row(
    left: &str,
    right: &str,
    path: &str,
    failure: impl FnOnce(&JsonValue, &JsonValue, &str) -> Result<String>,
    output: &Path,
) -> Result<()> {
    let left = json::parse_text(left)?;
    let right = json::parse_text(right)?;
    if let Err(error) = compare_with_limit(&left, &right, path, 997) {
        if error.is_assertion() {
            let report = JsonValue::parse(&failure(&left, &right, &error.message_json())?)?;
            let text = json::pretty_with_limit(&report, 0, 995)? + "\n";
            fs::write(output.join("failure.json"), text)?;
        }
        return Err(error.into());
    }
    Ok(())
}
pub(crate) fn options(
    exposure: bool,
) -> std::result::Result<ParsedOs, crate::argparse_scan::ArgumentError> {
    let mut specs = vec![
        OptionSpec::value("--reference", true),
        OptionSpec::value("--binary", true),
        OptionSpec::value("--output", true),
        OptionSpec::value("--runner", false),
        OptionSpec::value("--reference-runner", false),
    ];
    if exposure {
        specs.push(OptionSpec::value("--frames", false));
    }
    scan_os(&std::env::args_os().skip(1).collect::<Vec<_>>(), &specs)
}
pub(crate) fn prefix<'a>(options: &'a ParsedOs, name: &str) -> &'a OsStr {
    options
        .values
        .get(name)
        .map(OsString::as_os_str)
        .unwrap_or(OsStr::new(""))
}
