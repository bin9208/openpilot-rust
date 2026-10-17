//! Full independent fixed-input message-state gate; shared Python helpers remain pending.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn inflate(bytes: &[u8], name: &str, m: &serde_json::Value) -> Result<Vec<u8>> {
    let row = &m["corpora"][name];
    if Some(sha(bytes).as_str()) != row["gzip_sha256"].as_str() {
        return Err("compressed original corpus integrity mismatch".into());
    }
    let mut out = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(
            row["bytes"]
                .as_u64()
                .ok_or("missing original corpus length")?
                + 1,
        )
        .read_to_end(&mut out)?;
    if Some(sha(&out).as_str()) != row["sha256"].as_str() {
        return Err("original corpus integrity mismatch".into());
    }
    Ok(out)
}
fn pins(root: &Path, m: &serde_json::Value) -> Result<()> {
    for (path, hash) in m["sources"]
        .as_object()
        .ok_or("missing original source pins")?
    {
        if Some(sha(&fs::read(root.join(path))?).as_str()) != hash.as_str() {
            return Err(
                format!("{path} source changed; independently recapture original corpus").into(),
            );
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing source dependencies")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|n| matches!(n, "numpy" | "pycapnp" | "zstandard"))
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(records)? != m["lock"] {
        return Err(
            "message-state dependencies changed; independently recapture original corpus".into(),
        );
    }
    Ok(())
}
fn numeric(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Bool(v) => Ok(if v { 1. } else { 0. }),
        _ => Err("message-state subtraction requires a number".into()),
    }
}
/// The original comparator intentionally differs from diagnostic strict equality:
/// dictionary order is ignored, float error is inclusive, and Python numeric
/// equality (including bool/int) remains admitted for all nonfloat leaves.
pub fn compare(expected: &JsonValue, actual: &JsonValue) -> Result<f64> {
    let mut pending = vec![(expected.clone(), actual.clone())];
    let mut maximum = 0.0f64;
    while let Some((e, a)) = pending.pop() {
        match e.view() {
            JsonView::Object(fields) => {
                let JsonView::Object(observed) = a.view() else {
                    return Err("message-state dictionary required".into());
                };
                if fields.len() != observed.len() {
                    return Err("message-state dictionary keys differ".into());
                }
                for (key, value) in fields {
                    let observed = observed
                        .iter()
                        .find(|(k, _)| *k == key)
                        .ok_or("message-state dictionary keys differ")?;
                    pending.push((value, observed.1.clone()));
                }
            }
            JsonView::Array(expected) => {
                let actual = json::sequence(&a)?;
                if expected.len() != actual.len() {
                    return Err("message-state sequence lengths differ".into());
                }
                pending.extend(expected.into_iter().zip(actual));
            }
            JsonView::Float(value) => {
                let error = (value - numeric(&a)?).abs();
                if error
                    .partial_cmp(&1e-12)
                    .is_none_or(|v| v == std::cmp::Ordering::Greater)
                {
                    return Err("message-state float error exceeds 1e-12".into());
                }
                maximum = maximum.max(error);
            }
            _ => {
                if !json::equal(&e, &a) {
                    return Err("message-state scalar values differ".into());
                }
            }
        }
    }
    Ok(maximum)
}
fn parse(line: &str) -> Result<JsonValue> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for c in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            if c == '[' || c == '{' {
                depth += 1;
                if depth > 9997 {
                    return Err("Python JSON decoder nesting limit exceeded".into());
                }
            } else if c == ']' || c == '}' {
                depth = depth.saturating_sub(1);
            }
        }
    }
    Ok(JsonValue::parse(line)?)
}
fn text(b: Vec<u8>) -> Result<String> {
    let s = String::from_utf8(b)?;
    Ok(if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s
    })
}
fn open_text(path: &Path) -> Result<fs::File> {
    let file = fs::File::open(path)?;
    if file.metadata()?.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::IsADirectory,
            "message-state text path is a directory",
        )
        .into());
    }
    Ok(file)
}
fn outcomes(m: &serde_json::Value) -> Result<()> {
    for key in [
        "all_checks:True",
        "all_checks:False",
        "frequency_ok:True",
        "frequency_ok:False",
    ] {
        if m["outcomes"][key].as_u64().unwrap_or(0) == 0 {
            return Err(format!("original message-state outcome missing: {key}").into());
        }
    }
    if m["outcomes"]["error:zero_interval"].as_u64() != Some(1) {
        return Err("original message-state requires exactly one zero interval".into());
    }
    Ok(())
}
/// Replay every independently captured original frame, preserving file and child
/// ownership. No child timeout, inherited-stream decoding, or tolerance change.
pub fn check(root: &Path, binary: &Path, output: &Path, runner: &mut dyn Runner) -> Result<String> {
    let root = resolve_non_strict(root)?;
    let m: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/message-state/manifest.json"
    ))?;
    pins(&root, &m)?;
    let request = inflate(
        include_bytes!("../tests/fixtures/message-state/request.gz"),
        "request.gz",
        &m,
    )?;
    let expected = inflate(
        include_bytes!("../tests/fixtures/message-state/expected.gz"),
        "expected.gz",
        &m,
    )?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let request_path = output.join("request.jsonl");
    let expected_path = output.join("expected.jsonl");
    let actual_path = output.join("actual.jsonl");
    {
        // Original opens request first, expected second, then interleaves each frame.
        let mut requests = BufWriter::new(fs::File::create(&request_path)?);
        let mut references = BufWriter::new(fs::File::create(&expected_path)?);
        let mut a = request.split_inclusive(|&b| b == b'\n');
        let mut b = expected.split_inclusive(|&b| b == b'\n');
        loop {
            match (a.next(), b.next()) {
                (Some(a), Some(b)) => {
                    requests.write_all(a)?;
                    references.write_all(b)?;
                }
                (None, None) => break,
                _ => return Err("original message-state corpus lengths differ".into()),
            }
        }
        references.flush()?;
        requests.flush()?;
    }
    let mut spec = CommandSpec::new_os(
        vec![
            resolve_non_strict(binary)?.into_os_string(),
            resolve_non_strict(&request_path)?.into_os_string(),
            resolve_non_strict(&actual_path)?.into_os_string(),
        ],
        Duration::MAX,
    );
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    let child = runner.run(&spec)?;
    if child.code != 0 {
        return Err(format!("message-state candidate exited {}", child.code).into());
    }
    // File iteration uses universal newlines, not str.splitlines' Unicode separators.
    // The original opens both on-disk files after the child. Preserve even
    // child changes/removal of the reference file; do not compare a stale copy.
    let mut expected_file = open_text(&expected_path)?;
    let mut actual_file = open_text(&actual_path)?;
    let mut expected_bytes = vec![];
    let mut actual_bytes = vec![];
    expected_file.read_to_end(&mut expected_bytes)?;
    let expected = text(expected_bytes)?;
    actual_file.read_to_end(&mut actual_bytes)?;
    let actual = text(actual_bytes)?;
    let mut e = expected.split_inclusive('\n');
    let mut a = actual.split_inclusive('\n');
    let mut maximum = 0.0f64;
    let mut count = 0usize;
    loop {
        match (e.next(), a.next()) {
            (Some(e), Some(a)) => {
                maximum = maximum.max(compare(&parse(e)?, &parse(a)?)?);
                count += 1;
            }
            (None, None) => break,
            _ => return Err("message-state JSONL trace lengths differ".into()),
        }
    }
    outcomes(&m)?;
    let template = String::from_utf8(inflate(
        include_bytes!("../tests/fixtures/message-state/report.gz"),
        "report.gz",
        &m,
    )?)?;
    let report: serde_json::Value = serde_json::from_str(&template)?;
    if report["outcomes"] != m["outcomes"] {
        return Err("original message-state outcome report integrity mismatch".into());
    }
    let number = JsonValue::parse(&format!("{maximum:?}"))?.to_json()?;
    let needle = "\"maximum_absolute_error\": 0.0";
    if template.matches(needle).count() != 1 {
        return Err("original maximum error report field missing".into());
    }
    let updates = format!(
        "\"updates\": {}",
        report["updates"]
            .as_u64()
            .ok_or("original update count missing")?
    );
    if template.matches(&updates).count() != 1 {
        return Err("original update report field missing".into());
    }
    let report = template
        .replace(needle, &format!("\"maximum_absolute_error\": {number}"))
        .replace(&updates, &format!("\"updates\": {count}"));
    let printed = JsonValue::parse(&report)?.to_json()? + "\n";
    fs::write(output.join("report.json"), report)?;
    Ok(printed)
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
        println!("usage: check-message-state --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    print!(
        "{}",
        check(
            &root,
            Path::new(&args.values["--binary"]),
            Path::new(&args.values["--output"]),
            &mut NativeRunner
        )?
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn all_original_outcome_assertions_remain_required() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/message-state/manifest.json"
        ))
        .unwrap();
        for key in [
            "all_checks:True",
            "all_checks:False",
            "frequency_ok:True",
            "frequency_ok:False",
        ] {
            let mut changed = manifest.clone();
            changed["outcomes"][key] = serde_json::json!(0);
            assert!(super::outcomes(&changed).is_err(), "{key}");
        }
        for count in [0, 2] {
            let mut changed = manifest.clone();
            changed["outcomes"]["error:zero_interval"] = serde_json::json!(count);
            assert!(super::outcomes(&changed).is_err());
        }
        super::outcomes(&manifest).unwrap();
    }
}
