//! Complete original fixed-input amplifier register/failure policy gate.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
const SOURCE: &str = "openpilot/system/hardware/tici/amplifier.py";
const CHECKER: &str = "rust/tools/check_amplifier.py";
// Reviewed retired generator identity. The corpus remains data, never executed.
const CHECKER_SHA256: &str = "9849d44da31628ade4d9121d823a9aef13b4f94cc81633643f04ecde96174a0c";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Deserialize)]
struct Case {
    name: String,
    input: String,
    expected: String,
}
fn corpus() -> Result<(Vec<Case>, serde_json::Value)> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/amplifier-policy/manifest.json"
    ))?;
    let packed = include_bytes!("../tests/fixtures/amplifier-policy/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("amplifier compressed corpus mismatch".into());
    }
    let size = manifest["bytes"].as_u64().ok_or("missing corpus size")?;
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("amplifier corpus integrity mismatch".into());
    }
    let cases: Vec<Case> = serde_json::from_slice(&plain)?;
    if manifest["cases"].as_u64() != Some(cases.len() as u64) {
        return Err("amplifier corpus count mismatch".into());
    }
    Ok((cases, manifest))
}
fn parse(text: &str) -> Result<JsonValue> {
    json::parse_text(text)
}
fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![];
    }
    // Python str.splitlines also recognizes the Unicode line separators.
    let mut rows = text
        .split([
            '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
            '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if rows.last() == Some(&"") {
        rows.pop();
    }
    rows
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runners: &[OsString],
    runner: &mut dyn Runner,
) -> Result<String> {
    let (cases, manifest) = corpus()?;
    if manifest["sources"][SOURCE] != sha(&fs::read(root.join(SOURCE))?)
        || manifest["sources"][CHECKER] != CHECKER_SHA256
    {
        return Err("amplifier source changed; independently recapture policy corpus".into());
    }
    // During migration/reintroduction, do not accept an altered original tool.
    // Absence is explicit retirement of this fixed-input generator only.
    match fs::read(root.join(CHECKER)) {
        Ok(bytes) if sha(&bytes) != CHECKER_SHA256 => {
            return Err("amplifier checker changed; independently recapture policy corpus".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    // Preserve mkdir(exist_ok=False), including a pre-existing empty directory.
    // Create parents separately so a concurrent creator of the leaf cannot
    // accidentally turn this into exist_ok=True.
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let mut command = runners.to_vec();
    command.push(resolve_non_strict(binary)?.into_os_string());
    let mut spec = CommandSpec::new_os(command.clone(), Duration::MAX);
    spec.input = Some(
        cases
            .iter()
            .map(|r| format!("{}\n", r.input))
            .collect::<String>()
            .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = runner.run(&spec)?;
    let stdout = String::from_utf8(captured.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    // subprocess text mode validates stderr even though the original discards it.
    String::from_utf8(captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("amplifier candidate exited {}", captured.code).into());
    }
    let native = lines(&stdout)
        .into_iter()
        .map(parse)
        .collect::<Result<Vec<_>>>()?;
    if native.len() != cases.len() {
        return Err("amplifier candidate output count mismatch".into());
    }
    let mut reports = Vec::new();
    for (case, actual) in cases.iter().zip(native) {
        let expected = parse(&case.expected)?;
        let folder = output.join(&case.name);
        fs::create_dir(&folder)?;
        for (name, value) in [
            ("input.json", parse(&case.input)?),
            ("source.json", expected.clone()),
            ("native.json", actual.clone()),
        ] {
            fs::write(folder.join(name), json::pretty_with_limit(&value, 0, 996)?)?;
        }
        if !json::equal(&expected, &actual) {
            return Err(format!("amplifier mismatch: {}", case.name).into());
        }
        if let Some(value) = actual.get("outcome").and_then(|v| v.get("value")) {
            if !matches!(value.view(), JsonView::Bool(_)) {
                return Err("amplifier outcome value must be boolean".into());
            }
        }
        let events = expected.get("events").ok_or("missing expected events")?;
        let count = json::sequence(&events)?.len();
        reports.push(format!(
            "{{\"scenario\":{},\"result\":\"PASS\",\"events\":{count}}}",
            serde_json::to_string(&case.name)?
        ));
    }
    let argv = command
        .iter()
        .map(|v| python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let result = parse(&format!("{{\"result\":\"PASS\",\"cases\":[{}],\"argv\":[{argv}],\"exit\":{},\"binary_sha256\":\"{}\",\"source_sha256\":{}}}", reports.join(","), captured.code, sha(&fs::read(binary)?), manifest["sources"][SOURCE]))?;
    fs::write(output.join("result.json"), json::pretty(&result, 0)?)?;
    Ok(format!(
        "{} full-source register/mask/order/debug/failure cases PASS\n",
        cases.len()
    ))
}
pub fn cli() -> Result<()> {
    let parsed = scan_os_with_positionals(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[OptionSpec::append("--runner")],
        2,
    )?;
    if parsed.help {
        println!("Usage: check-amplifier-policy BINARY OUTPUT [--runner ARG]...");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    print!(
        "{}",
        check(
            &root,
            Path::new(&parsed.positionals[0]),
            Path::new(&parsed.positionals[1]),
            parsed
                .append_values
                .get("--runner")
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &mut NativeRunner
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn original_decoder_boundary_checks_discarded_duplicates() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/amplifier-policy/depth.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let depth = case["total_depth"].as_u64().unwrap() as usize;
            let text = format!(
                "{{\"v\":{}0{},\"v\":1}}",
                "[".repeat(depth - 1),
                "]".repeat(depth - 1)
            );
            assert_eq!(parse(&text).is_ok(), case["error"].is_null(), "{depth}");
        }
    }
}
