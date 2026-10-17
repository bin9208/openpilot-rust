//! Fixed original pose/calibration/actuation/camera checker; generic runtime Python remains live.
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
const CHECKER: &str = "rust/tools/check_selfdrive_helpers.py";
const CHECKER_SHA: &str = "9a093795c8eeaaefd849672fcce7eff0058e600d4b3857a7ee05129afb447781";
const LIVE_SOURCES: [&str; 15] = [
    "opendbc_repo/opendbc/car/__init__.py",
    "opendbc_repo/opendbc/car/car.capnp",
    "opendbc_repo/opendbc/car/interfaces.py",
    "opendbc_repo/opendbc/car/lateral.py",
    "openpilot/cereal/__init__.py",
    "openpilot/cereal/custom.capnp",
    "openpilot/cereal/deprecated.capnp",
    "openpilot/cereal/include/c++.capnp",
    "openpilot/cereal/log.capnp",
    "openpilot/common/realtime.py",
    "openpilot/common/transformations/orientation.py",
    "openpilot/common/transformations/transformations.py",
    "openpilot/selfdrive/locationd/helpers.py",
    "openpilot/selfdrive/selfdrived/camera_config.py",
    "openpilot/selfdrive/selfdrived/helpers.py",
];
#[derive(Deserialize)]
struct Corpus {
    input: String,
    source: String,
    expected: String,
    report_file: String,
    report_line: String,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<(Corpus, serde_json::Value)> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/selfdrive-helpers/manifest.json"
    ))?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["cases"] != 10241
        || manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|v| v.starts_with("3.12."))
    {
        return Err("selfdrive helpers source/interpreter provenance changed".into());
    }
    let sources = manifest["sources"]
        .as_object()
        .ok_or("missing selfdrive helpers source pins")?;
    if sources.len() != LIVE_SOURCES.len() + 1
        || LIVE_SOURCES.iter().any(|name| !sources.contains_key(*name))
    {
        return Err("selfdrive helpers live source pin set changed".into());
    }
    for (name, pin) in sources {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(
                format!("{name} changed; independently recapture selfdrive helpers").into(),
            );
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let dependencies = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing source dependency lock")?
        .iter()
        .filter(|record| {
            record
                .get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| matches!(name, "numpy" | "pycapnp"))
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(dependencies)? != manifest["dependency_lock"] {
        return Err("helpers NumPy/pycapnp source lock changed".into());
    }
    let packed = include_bytes!("../tests/fixtures/selfdrive-helpers/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("selfdrive helpers compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing selfdrive helpers corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("selfdrive helpers corpus changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.input.lines().count() != 10241 || corpus.expected.lines().count() != 10241 {
        return Err("selfdrive helpers corpus count changed".into());
    }
    Ok((corpus, manifest))
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn number(value: &JsonValue) -> Result<f64> {
    let result = match value.view() {
        JsonView::Float(value) => value,
        JsonView::Integer(value) => {
            let result = value.parse::<f64>()?;
            if !result.is_finite() {
                return Err("integer too large to convert to float".into());
            }
            result
        }
        JsonView::Bool(value) => f64::from(u8::from(value)),
        _ => return Err("math.isclose requires a real number".into()),
    };
    Ok(result)
}
/// The original comparator branches on expected type. NaN expects null; float
/// leaves use math.isclose(2e-12, 2e-12), while other leaves use Python equality.
pub fn compare(expected: &JsonValue, actual: &JsonValue) -> Result<()> {
    let mut pending = vec![(expected.clone(), actual.clone())];
    while let Some((expected, actual)) = pending.pop() {
        match expected.view() {
            JsonView::Object(expected) => {
                let JsonView::Object(actual) = actual.view() else {
                    return Err("helpers expected object".into());
                };
                if expected.len() != actual.len() {
                    return Err("helpers object keys differ".into());
                }
                for (key, value) in expected {
                    let other = actual
                        .iter()
                        .find(|(name, _)| *name == key)
                        .ok_or("helpers object keys differ")?;
                    pending.push((value, other.1.clone()));
                }
            }
            JsonView::Array(expected) => {
                // Python zip accepts any iterable; dict keys and string
                // characters are admitted before recursive comparisons.
                let actual = json::sequence(&actual)?;
                if expected.len() != actual.len() {
                    return Err("helpers sequence lengths differ".into());
                }
                pending.extend(expected.into_iter().zip(actual));
            }
            JsonView::Float(expected) => {
                if expected.is_nan() {
                    if !matches!(actual.view(), JsonView::Null) {
                        return Err("helpers NaN requires null".into());
                    }
                } else {
                    let actual = number(&actual)?;
                    let close = expected == actual
                        || (expected.is_finite() && actual.is_finite() && {
                            let difference = (expected - actual).abs();
                            difference <= 2e-12
                                || difference <= 2e-12 * expected.abs()
                                || difference <= 2e-12 * actual.abs()
                        });
                    if !close {
                        return Err("helpers float exceeds original numeric tolerance".into());
                    }
                }
            }
            _ => {
                if !json::equal(&expected, &actual) {
                    return Err("helpers scalar differs".into());
                }
            }
        }
    }
    Ok(())
}

pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let (corpus, _) = corpus(root)?;
    // Original exist_ok=False: existing evidence must never be overwritten.
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    fs::write(output.join("input.jsonl"), &corpus.input)?;
    fs::write(output.join("source.json"), &corpus.source)?;
    let mut spec = CommandSpec::new_os(
        vec![resolve_non_strict(binary)?.into_os_string()],
        Duration::from_secs(60),
    );
    spec.input = Some(corpus.input.into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let child = runner.run(&spec)?;
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join("native.jsonl"), &stdout)?;
    fs::write(output.join("native.stderr"), &stderr)?;
    if child.code != 0 {
        return Err(format!("selfdrive helpers candidate exited {}", child.code).into());
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
    if actual.len() != 10241 {
        return Err("selfdrive helpers candidate row count differs".into());
    }
    for (expected, actual) in corpus.expected.lines().zip(actual) {
        compare(&json::parse_text(expected)?, &actual)?;
    }
    let binary_sha = sha(&fs::read(binary)?);
    fs::write(
        output.join("report.json"),
        corpus.report_file.replace("@@BINARY_SHA256@@", &binary_sha),
    )?;
    printed.write_all(corpus.report_line.as_bytes())?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--native", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-selfdrive-helpers --native EXAMPLE --output DIRECTORY");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&args.values["--native"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
