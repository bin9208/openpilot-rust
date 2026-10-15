//! Independent fixed-corpus widget/navigation checks; generic Python UI remains pending.
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
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Widgets,
    Navigation,
}
impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::Widgets => "widgets",
            Self::Navigation => "navigation",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Widgets => "widget",
            Self::Navigation => "navigation",
        }
    }
    fn bytes(self, source: bool) -> &'static [u8] {
        match (self, source) {
            (Self::Widgets, false) => {
                include_bytes!("../tests/fixtures/ui-headless/widget-input.json.gz")
            }
            (Self::Widgets, true) => {
                include_bytes!("../tests/fixtures/ui-headless/widget-source.json.gz")
            }
            (Self::Navigation, false) => {
                include_bytes!("../tests/fixtures/ui-headless/navigation-input.json.gz")
            }
            (Self::Navigation, true) => {
                include_bytes!("../tests/fixtures/ui-headless/navigation-source.json.gz")
            }
        }
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(kind: Kind, source: bool, manifest: &serde_json::Value) -> Result<Vec<u8>> {
    let name = format!(
        "{}-{}.json.gz",
        kind.prefix(),
        if source { "source" } else { "input" }
    );
    let entry = &manifest["corpora"][&name];
    let bytes = kind.bytes(source);
    if Some(sha(bytes).as_str()) != entry["gzip_sha256"].as_str() {
        return Err("compressed original corpus integrity mismatch".into());
    }
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(
            entry["bytes"]
                .as_u64()
                .ok_or("missing original corpus length")?
                + 1,
        )
        .read_to_end(&mut plain)?;
    if Some(sha(&plain).as_str()) != entry["sha256"].as_str() {
        return Err("original corpus integrity mismatch".into());
    }
    Ok(plain)
}
fn pins(root: &Path, kind: Kind, manifest: &serde_json::Value) -> Result<()> {
    for (path, hash) in manifest["sources"][kind.key()]
        .as_object()
        .ok_or("missing original source pins")?
    {
        if Some(sha(&fs::read(root.join(path))?).as_str()) != hash.as_str() {
            return Err(
                format!("{path} source changed; independently recapture original corpus").into(),
            );
        }
    }
    if matches!(kind, Kind::Navigation) {
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
        let matches = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing source packages")?
            .iter()
            .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some("numpy"))
            .collect::<Vec<_>>();
        if serde_json::to_value(matches)? != manifest["numpy_lock"] {
            return Err("numpy source changed; independently recapture original corpus".into());
        }
    }
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn parse(text: &str) -> Result<JsonValue> {
    // Whole unchanged Python3.12.15 CLI admits total depth9997; even ignored
    // fields must be decoded and cannot bypass the original decoder limit.
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for ch in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            if ch == '[' || ch == '{' {
                depth += 1;
                if depth > 9997 {
                    return Err("Python JSON nesting limit exceeded".into());
                }
            } else if ch == ']' || ch == '}' {
                depth = depth.saturating_sub(1);
            }
        }
    }
    Ok(JsonValue::parse(text)?)
}
fn numeric(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        _ => Err("navigation subtraction requires a numeric value".into()),
    }
}
fn compare(expected: &JsonValue, actual: &JsonValue, kind: Kind) -> Result<()> {
    if matches!(kind, Kind::Widgets) {
        if !json::equal(expected, actual) {
            return Err("source/native widget transitions differ".into());
        }
    } else {
        let expected = json::sequence(expected)?;
        let actual = json::sequence(actual)?;
        if expected.len() != actual.len() {
            return Err("source/native navigation trace lengths differ".into());
        }
        for (e, a) in expected.iter().zip(&actual) {
            let JsonView::Object(fields) = e.view() else {
                return Err("invalid original navigation frame".into());
            };
            for (key, value) in fields {
                let key = String::from_utf16(
                    &key.iter()
                        .map(|&c| u16::try_from(c))
                        .collect::<std::result::Result<Vec<_>, _>>()?,
                )?;
                let observed = a.get(&key).ok_or("missing navigation field")?;
                // Original compares all expected keys, including counts/bools,
                // but intentionally ignores extra candidate keys.
                if (numeric(&observed)? - numeric(&value)?)
                    .abs()
                    .partial_cmp(&1e-6)
                    != Some(std::cmp::Ordering::Less)
                {
                    return Err("source/native navigation frames differ".into());
                }
            }
        }
    }
    Ok(())
}
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut out = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    OsString::from_vec(out).into()
}
/// Execute only the independent original CLI corpus, with inherited environment
/// and captured strict UTF-8 streams. Original execution is unbounded.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
) -> Result<String> {
    let root = resolve_non_strict(root)?;
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-headless/original-cases.json"
    ))?;
    pins(&root, kind, &manifest)?;
    let input = corpus(kind, false, &manifest)?;
    let source = corpus(kind, true, &manifest)?;
    fs::create_dir_all(output)?;
    fs::write(output.join(format!("{}-input.json", kind.prefix())), &input)?;
    fs::write(
        output.join(format!("{}-source.json", kind.prefix())),
        &source,
    )?;
    let mut command = CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
    command.input = Some(input);
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let result = runner.run(&command)?;
    let stdout = text(result.stdout)?;
    let _stderr = text(result.stderr)?;
    if result.code != 0 {
        return Err(format!("{} candidate exited {}", kind.key(), result.code).into());
    }
    fs::write(
        output.join(format!("{}-native.json", kind.prefix())),
        &stdout,
    )?;
    compare(
        &parse(std::str::from_utf8(&source)?)?,
        &parse(&stdout)?,
        kind,
    )?;
    Ok(match kind {
        Kind::Widgets => "PASS: 800 source/native widget frames; clipping, multitouch, wake, disabled, hidden and invalid-touch transitions\n",
        Kind::Navigation => "PASS: 480 source/native navigation frames; show/swipe/cancel/blocked/programmatic dismiss and callback counts\n",
    }.into())
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
            "usage: check-ui-{} --binary BINARY --output OUTPUT",
            kind.key()
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
