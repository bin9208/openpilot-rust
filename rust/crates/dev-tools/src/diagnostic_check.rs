//! Source-pinned fixed-input diagnostics gates. Generic runtime helpers remain pending.
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
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Runtime,
    ModelTiming,
    Communication,
}
struct Scene {
    name: &'static str,
    input: &'static [u8],
    source: &'static [u8],
}
macro_rules! scene {
    ($name:literal) => {
        Scene {
            name: $name,
            input: include_bytes!(concat!(
                "../tests/fixtures/diagnostic-check/",
                $name,
                "-input.gz"
            )),
            source: include_bytes!(concat!(
                "../tests/fixtures/diagnostic-check/",
                $name,
                "-source.gz"
            )),
        }
    };
}
const RUNTIME: [Scene; 3] = [
    scene!("aggregation"),
    scene!("rounding"),
    scene!("integer-and-overflow"),
];
const TIMING: [Scene; 1] = [scene!("timing")];
const COMMUNICATION: [Scene; 1] = [scene!("communication")];
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn inflate(bytes: &[u8], name: &str, manifest: &serde_json::Value) -> Result<Vec<u8>> {
    let row = &manifest["corpora"][name];
    if Some(sha(bytes).as_str()) != row["gzip_sha256"].as_str() {
        return Err("compressed original corpus integrity mismatch".into());
    }
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(row["bytes"].as_u64().ok_or("missing corpus length")? + 1)
        .read_to_end(&mut plain)?;
    if Some(sha(&plain).as_str()) != row["sha256"].as_str() {
        return Err("original corpus integrity mismatch".into());
    }
    Ok(plain)
}
fn pins(root: &Path, kind: Kind, manifest: &serde_json::Value) -> Result<()> {
    let key = match kind {
        Kind::Runtime => "runtime",
        Kind::ModelTiming => "model_timing",
        Kind::Communication => "communication",
    };
    for (path, expected) in manifest["sources"][key]
        .as_object()
        .ok_or("missing original source pins")?
    {
        if Some(sha(&fs::read(root.join(path))?).as_str()) != expected.as_str() {
            return Err(
                format!("{path} source changed; independently recapture original corpus").into(),
            );
        }
    }
    if matches!(kind, Kind::ModelTiming) {
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
        let records = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing source dependencies")?
            .iter()
            .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some("pytest"))
            .collect::<Vec<_>>();
        if serde_json::to_value(records)? != manifest["pytest_lock"] {
            return Err("pytest source changed; independently recapture original corpus".into());
        }
    }
    if matches!(kind, Kind::Communication) {
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
        let records = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing source dependencies")?
            .iter()
            .filter(|p| {
                p.get("name")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| matches!(name, "numpy" | "pycapnp" | "zstandard"))
            })
            .collect::<Vec<_>>();
        if serde_json::to_value(records)? != manifest["communication_lock"] {
            return Err(
                "communication dependencies changed; independently recapture original corpus"
                    .into(),
            );
        }
    }
    Ok(())
}
/// Match the original JSON comparator: exact Python types, object insertion
/// order, scalar values, and double bits (all NaN payloads compare equal).
pub fn compare_exact(expected: &JsonValue, actual: &JsonValue) -> Result<()> {
    let mut pending = vec![(expected.clone(), actual.clone())];
    while let Some((e, a)) = pending.pop() {
        let equal = match (e.view(), a.view()) {
            (JsonView::Null, JsonView::Null) => true,
            (JsonView::Bool(e), JsonView::Bool(a)) => e == a,
            (JsonView::Integer(_), JsonView::Integer(_)) => json::equal(&e, &a),
            (JsonView::Float(e), JsonView::Float(a)) => {
                e.to_bits() == a.to_bits() || e.is_nan() && a.is_nan()
            }
            (JsonView::Text(e), JsonView::Text(a)) => e == a,
            (JsonView::Array(e), JsonView::Array(a)) => {
                if e.len() != a.len() {
                    return Err("diagnostic trace lengths differ".into());
                }
                pending.extend(e.into_iter().zip(a));
                true
            }
            (JsonView::Object(e), JsonView::Object(a)) => {
                if e.len() != a.len() {
                    return Err("diagnostic object keys differ".into());
                }
                for ((ek, ev), (ak, av)) in e.into_iter().zip(a) {
                    if ek != ak {
                        return Err("diagnostic object key order differs".into());
                    }
                    pending.push((ev, av));
                }
                true
            }
            _ => false,
        };
        if !equal {
            return Err("original/native diagnostic types or values differ".into());
        }
    }
    Ok(())
}
fn text(b: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(b)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn depth(value: &JsonValue, limit: usize) -> Result<()> {
    let mut pending = vec![(value.clone(), 0usize)];
    while let Some((v, d)) = pending.pop() {
        match v.view() {
            JsonView::Array(values) => {
                if d >= limit {
                    return Err("Python JSON encoder nesting limit exceeded".into());
                }
                pending.extend(values.into_iter().map(|v| (v, d + 1)));
            }
            JsonView::Object(values) => {
                if d >= limit {
                    return Err("Python JSON encoder nesting limit exceeded".into());
                }
                pending.extend(values.into_iter().map(|(_, v)| (v, d + 1)));
            }
            _ => {}
        }
    }
    Ok(())
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
fn lines(text: &str) -> Vec<&str> {
    // Python str.splitlines also recognizes Unicode line/paragraph separators,
    // vertical tab, form feed and information separators. CR was normalized.
    let mut lines = vec![];
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if matches!(
            c,
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            lines.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}
fn parse_lines(text: &str) -> Result<Vec<JsonValue>> {
    lines(text).into_iter().map(parse).collect()
}
fn compare_rows(expected: Vec<JsonValue>, actual: Vec<JsonValue>) -> Result<()> {
    if expected.len() != actual.len() {
        return Err("original/native diagnostic trace lengths differ".into());
    }
    for (e, a) in expected.iter().zip(&actual) {
        compare_exact(e, a)?;
    }
    Ok(())
}
fn array_json(values: &[JsonValue]) -> Result<String> {
    // The original C encoder allows total container depth9997. The outer array
    // is introduced by the driver's parsed JSONL list, after each line decoded.
    let mut out = String::from("[");
    for (i, v) in values.iter().enumerate() {
        depth(v, 9996)?;
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&v.to_json()?);
    }
    out.push(']');
    Ok(out)
}
/// Execute the complete independent original fixed corpus, preserving differing
/// artifact phases and original unbounded child execution/captured streams.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
) -> Result<String> {
    let root = resolve_non_strict(root)?;
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/diagnostic-check/manifest.json"
    ))?;
    pins(&root, kind, &manifest)?;
    let scenes = match kind {
        Kind::Runtime => RUNTIME.as_slice(),
        Kind::ModelTiming => TIMING.as_slice(),
        Kind::Communication => COMMUNICATION.as_slice(),
    };
    let corpora = scenes
        .iter()
        .map(|s| {
            Ok((
                inflate(s.input, &format!("{}-input.gz", s.name), &manifest)?,
                inflate(s.source, &format!("{}-source.gz", s.name), &manifest)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let timing = matches!(kind, Kind::ModelTiming);
    let (resolved_binary, output) = if timing {
        let binary = resolve_non_strict(binary)?;
        let output = resolve_non_strict(output)?;
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(&output)?;
        (Some(binary), output)
    } else {
        fs::create_dir_all(output)?;
        (None, output.to_path_buf())
    };
    for (scene, (input, source)) in scenes.iter().zip(corpora) {
        if timing {
            fs::write(output.join("inputs.jsonl"), &input)?;
        }
        let binary = if let Some(path) = &resolved_binary {
            path.clone()
        } else {
            resolve_non_strict(binary)?
        };
        let mut spec = CommandSpec::new_os(vec![binary.into_os_string()], Duration::MAX);
        spec.input = Some(input.clone());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let result = runner.run(&spec)?;
        let stdout = text(result.stdout)?;
        let _stderr = text(result.stderr)?;
        if result.code != 0 {
            return Err(format!("diagnostic candidate exited {}", result.code).into());
        }
        if timing {
            fs::write(output.join("rust.jsonl"), &stdout)?;
            fs::write(output.join("source.jsonl"), &source)?;
            compare_rows(
                parse_lines(std::str::from_utf8(&source)?)?,
                parse_lines(&stdout)?,
            )?;
        } else if matches!(kind, Kind::Communication) {
            let actual = parse_lines(&stdout)?;
            fs::write(output.join("input.jsonl"), &input)?;
            fs::write(output.join("expected.json"), &source)?;
            let encoded = array_json(&actual)?;
            fs::write(output.join("actual.json"), encoded)?;
            compare_rows(
                json::sequence(&JsonValue::parse(std::str::from_utf8(&source)?)?)?,
                actual,
            )?;
        } else {
            let actual = parse_lines(&stdout)?;
            fs::write(output.join(format!("{}-input.jsonl", scene.name)), &input)?;
            let encoded = array_json(&actual)?;
            fs::write(output.join(format!("{}-actual.json", scene.name)), encoded)?;
            fs::write(
                output.join(format!("{}-expected.json", scene.name)),
                &source,
            )?;
            compare_rows(
                json::sequence(&JsonValue::parse(std::str::from_utf8(&source)?)?)?,
                actual,
            )?;
        }
    }
    let (report, filename, key) = if timing {
        (
            include_bytes!("../tests/fixtures/diagnostic-check/model_timing-report.gz").as_slice(),
            "model_timing-report.gz",
            "model_timing_stdout",
        )
    } else if matches!(kind, Kind::Communication) {
        (
            include_bytes!("../tests/fixtures/diagnostic-check/communication-report.gz").as_slice(),
            "communication-report.gz",
            "communication_stdout",
        )
    } else {
        (
            include_bytes!("../tests/fixtures/diagnostic-check/runtime-report.gz").as_slice(),
            "runtime-report.gz",
            "runtime_stdout",
        )
    };
    fs::write(
        output.join("report.json"),
        inflate(report, filename, &manifest)?,
    )?;
    Ok(manifest[key]
        .as_str()
        .ok_or("missing original stdout")?
        .into())
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!(
            "usage: check-{} --binary BINARY --output OUTPUT",
            match kind {
                Kind::Runtime => "runtime-diagnostics",
                Kind::ModelTiming => "model-timing",
                Kind::Communication => "communication-snapshot",
            }
        );
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
            kind,
            &mut NativeRunner
        )?
    );
    Ok(())
}
