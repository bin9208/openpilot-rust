//! Source-pinned independent original scroll/scroller CLI corpora.
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
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Scroll,
    Scroller,
}
impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::Scroll => "scroll",
            Self::Scroller => "scroller",
        }
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn compressed(name: &str) -> Result<&'static [u8]> {
    match name {
        "scroll-0-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-0-input.json.gz"
        )),
        "scroll-0-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-0-source.json.gz"
        )),
        "scroll-0-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-0-debug.txt.gz"
        )),
        "scroll-1-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-1-input.json.gz"
        )),
        "scroll-1-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-1-source.json.gz"
        )),
        "scroll-1-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-1-debug.txt.gz"
        )),
        "scroll-2-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-2-input.json.gz"
        )),
        "scroll-2-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-2-source.json.gz"
        )),
        "scroll-2-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroll-2-debug.txt.gz"
        )),
        "scroller-0-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-0-input.json.gz"
        )),
        "scroller-0-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-0-source.json.gz"
        )),
        "scroller-0-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-0-debug.txt.gz"
        )),
        "scroller-1-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-1-input.json.gz"
        )),
        "scroller-1-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-1-source.json.gz"
        )),
        "scroller-1-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-1-debug.txt.gz"
        )),
        "scroller-2-input.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-2-input.json.gz"
        )),
        "scroller-2-source.json.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-2-source.json.gz"
        )),
        "scroller-2-debug.txt.gz" => Ok(include_bytes!(
            "../tests/fixtures/ui-scroll/scroller-2-debug.txt.gz"
        )),
        _ => Err("unknown original corpus".into()),
    }
}
fn corpus(name: &str, manifest: &serde_json::Value) -> Result<Vec<u8>> {
    let bytes = compressed(name)?;
    let row = &manifest[name];
    if row["gzip_sha256"].as_str() != Some(sha(bytes).as_str()) {
        return Err("compressed corpus integrity mismatch".into());
    }
    let size = row["bytes"]
        .as_u64()
        .ok_or("missing original corpus length")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || row["sha256"].as_str() != Some(sha(&plain).as_str()) {
        return Err("original corpus integrity mismatch".into());
    }
    Ok(plain)
}
fn pins(root: &Path, kind: Kind) -> Result<()> {
    let pins: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui-scroll/source-pins.json"))?;
    for (path, hash) in pins[kind.key()].as_object().ok_or("missing source pins")? {
        if hash.as_str() != Some(sha(&fs::read(root.join(path))?).as_str()) {
            return Err(format!("{path} changed; independently recapture original corpus").into());
        }
    }
    if matches!(kind, Kind::Scroller) {
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
        let expected: toml::Value =
            toml::from_str(include_str!("../tests/fixtures/ui-scroll/numpy-lock.toml"))?;
        let entries = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing packages")?;
        let found = entries
            .iter()
            .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some("numpy"))
            .collect::<Vec<_>>();
        if found.len() != 1 || *found[0] != expected {
            return Err("numpy dependency changed; independently recapture original corpus".into());
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
        _ => Err("scroll subtraction requires a numeric value".into()),
    }
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

fn compare(expected: &JsonValue, actual: &JsonValue, kind: Kind) -> Result<()> {
    if matches!(kind, Kind::Scroll) {
        let e = json::sequence(expected)?;
        let a = json::sequence(actual)?;
        if e.len() != a.len() {
            return Err("scroll frame lengths differ".into());
        }
        for (e, a) in e.iter().zip(&a) {
            for key in ["state", "touch_valid"] {
                if !json::equal(
                    &e.get(key).ok_or("missing source field")?,
                    &a.get(key).ok_or("missing candidate field")?,
                ) {
                    return Err("scroll discrete states differ".into());
                }
            }
            for (key, bound) in [("offset", 1e-5), ("velocity", 1e-8)] {
                if (numeric(&a.get(key).ok_or("missing candidate field")?)?
                    - numeric(&e.get(key).ok_or("missing source field")?)?)
                .abs()
                .partial_cmp(&bound)
                    != Some(std::cmp::Ordering::Less)
                {
                    return Err("scroll numeric states differ".into());
                }
            }
        }
        return Ok(());
    }
    let mut pending = vec![(expected.clone(), actual.clone())];
    while let Some((e, a)) = pending.pop() {
        match e.view() {
            JsonView::Object(fields) => {
                let JsonView::Object(observed) = a.view() else {
                    return Err("scroller mapping required".into());
                };
                if fields.len() != observed.len() {
                    return Err("scroller keys differ".into());
                }
                for (key, v) in fields {
                    let observed = observed
                        .iter()
                        .find(|(k, _)| *k == key)
                        .ok_or("missing scroller key")?;
                    pending.push((v, observed.1.clone()));
                }
            }
            JsonView::Array(values) => {
                let observed = json::sequence(&a)?;
                if values.len() != observed.len() {
                    return Err("scroller lengths differ".into());
                }
                pending.extend(values.into_iter().zip(observed));
            }
            JsonView::Float(v) => {
                if (numeric(&a)? - v)
                    .abs()
                    .partial_cmp(&1e-4)
                    .is_none_or(|o| o == std::cmp::Ordering::Greater)
                {
                    return Err("scroller numeric states differ".into());
                }
            }
            _ => {
                if !json::equal(&e, &a) {
                    return Err("scroller states differ".into());
                }
            }
        }
    }
    Ok(())
}
/// Compare the fixed original corpus. Debug bytes precede each scene's files and child.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    debug: bool,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    fs::create_dir_all(output)?;
    let root = resolve_non_strict(root)?;
    pins(&root, kind)?;
    let integrity: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui-scroll/integrity.json"))?;
    let mut scenes = vec![];
    for i in 0..3 {
        let prefix = format!("{}-{i}", kind.key());
        scenes.push((
            corpus(&format!("{prefix}-input.json.gz"), &integrity)?,
            corpus(&format!("{prefix}-source.json.gz"), &integrity)?,
            corpus(&format!("{prefix}-debug.txt.gz"), &integrity)?,
        ));
    }
    for (i, (input, source, debug_bytes)) in scenes.into_iter().enumerate() {
        if debug {
            printed.write_all(&debug_bytes)?;
        }
        let prefix = format!("{}-{i}", kind.key());
        fs::write(output.join(format!("{prefix}-input.json")), &input)?;
        fs::write(output.join(format!("{prefix}-source.json")), &source)?;
        let mut command =
            CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
        command.input = Some(input);
        command.stdout_limit = usize::MAX;
        command.stderr_limit = usize::MAX;
        let result = runner.run(&command)?;
        let stdout = text(result.stdout)?;
        let _stderr = text(result.stderr)?;
        if result.code != 0 {
            return Err(format!("candidate exited {}", result.code).into());
        }
        fs::write(output.join(format!("{prefix}-native.json")), &stdout)?;
        compare(
            &parse(std::str::from_utf8(&source)?)?,
            &parse(&stdout)?,
            kind,
        )?;
    }
    let message = match kind {
        Kind::Scroll => {
            "PASS: 4500 source/native scroll frames; horizontal/vertical, bounce/snap, disabled, multi-slot and content changes\n"
        }
        Kind::Scroller => {
            "PASS: 720 source/native scroller frames; snap, reorder/lift, smooth/instant scroll, hiding and child click gates\n"
        }
    };
    printed.write_all(message.as_bytes())?;
    Ok(())
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
    check(
        &root,
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--output"]),
        kind,
        std::env::var_os("DEBUG_SCROLL").as_deref() == Some(OsStr::new("1")),
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
