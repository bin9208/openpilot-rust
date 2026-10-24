//! Independently captured fixed command-reader gates; generic Bluetooth runtime remains pending.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Reference,
    JsonBoundary,
}
impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::JsonBoundary => "json-boundary",
        }
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bytes(kind: Kind, lane: &str) -> Result<&'static [u8]> {
    match (kind, lane) {
        (Kind::Reference, "operations") => Ok(include_bytes!(
            "../tests/fixtures/command-reference/reference-operations.gz"
        )),
        (Kind::Reference, "expected") => Ok(include_bytes!(
            "../tests/fixtures/command-reference/reference-expected.gz"
        )),
        (Kind::JsonBoundary, "operations") => Ok(include_bytes!(
            "../tests/fixtures/command-reference/json-boundary-operations.gz"
        )),
        (Kind::JsonBoundary, "expected") => Ok(include_bytes!(
            "../tests/fixtures/command-reference/json-boundary-expected.gz"
        )),
        _ => Err("unknown independent command corpus".into()),
    }
}
fn corpus(kind: Kind, lane: &str, manifest: &serde_json::Value) -> Result<Vec<u8>> {
    let name = format!("{}-{lane}.gz", kind.key());
    let row = &manifest["corpora"][name];
    let bytes = bytes(kind, lane)?;
    if row["gzip_sha256"].as_str() != Some(sha(bytes).as_str()) {
        return Err("compressed command corpus integrity mismatch".into());
    }
    let size = row["bytes"].as_u64().ok_or("missing command corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || row["sha256"].as_str() != Some(sha(&plain).as_str()) {
        return Err("command corpus integrity mismatch".into());
    }
    Ok(plain)
}
fn pins(root: &Path, manifest: &serde_json::Value) -> Result<()> {
    for (path, hash) in manifest["sources"]
        .as_object()
        .ok_or("missing command source pins")?
    {
        if hash.as_str() != Some(sha(&fs::read(root.join(path))?).as_str()) {
            return Err(
                format!("{path} changed; independently recapture command-reader fixtures").into(),
            );
        }
    }
    for path in manifest["absent_sources"]
        .as_array()
        .ok_or("missing namespace source contract")?
    {
        let name = path.as_str().ok_or("invalid namespace source")?;
        if root.join(name).try_exists()? {
            return Err(format!(
                "{name} was added; independently recapture command-reader fixtures"
            )
            .into());
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

// Python normalizes import search paths lexically, preserving symlink aliases.
fn import_root(root: &Path) -> Result<PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let raw = absolute.as_os_str().as_bytes();
    let mut parts = vec![];
    for part in raw.split(|b| *b == b'/') {
        if part.is_empty() || part == b"." {
            continue;
        }
        if part == b".." {
            parts.pop();
        } else {
            parts.push(part);
        }
    }
    let mut result = if raw.starts_with(b"//") && !raw.starts_with(b"///") {
        b"//".to_vec()
    } else {
        b"/".to_vec()
    };
    for part in parts {
        if !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    Ok(OsString::from_vec(result).into())
}
fn replay(output: &Path, operations: &[u8]) -> Result<()> {
    let operations: serde_json::Value = serde_json::from_slice(operations)?;
    let mut request: Option<BufWriter<fs::File>> = None;
    for op in operations
        .as_array()
        .ok_or("command source operations must be a sequence")?
    {
        match op["operation"]
            .as_str()
            .ok_or("missing command operation")?
        {
            "request-open" => {
                if request.is_some() {
                    return Err("command request opened twice".into());
                }
                request = Some(BufWriter::new(fs::File::create(
                    output.join("request.jsonl"),
                )?));
            }
            "request-append" => {
                request
                    .as_mut()
                    .ok_or("command request not open")?
                    .write_all(
                        op["text"]
                            .as_str()
                            .ok_or("missing original request text")?
                            .as_bytes(),
                    )?;
            }
            "request-close" => {
                request.take().ok_or("command request not open")?.flush()?;
            }
            "write" => {
                let path = op["path"].as_str().ok_or("missing original journal path")?;
                fs::write(
                    output.join(path),
                    op["text"].as_str().ok_or("missing original journal text")?,
                )?;
            }
            _ => return Err("unknown original command operation".into()),
        }
    }
    if request.is_some() {
        return Err("original command request was not closed".into());
    }
    Ok(())
}
/// Execute only the original deterministic CLI inputs, replaying every source
/// journal write and preserving the child's ownership of its separate journal.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let root = import_root(root)?;
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/command-reference/manifest.json"
    ))?;
    pins(&root, &manifest)?;
    let operations = corpus(kind, "operations", &manifest)?;
    let expected = corpus(kind, "expected", &manifest)?;
    let expected = json::sequence(&parse(std::str::from_utf8(&expected)?)?)?;
    let output = lexical(output);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output)?;
    fs::create_dir(output.join(if matches!(kind, Kind::Reference) {
        "original"
    } else {
        "source"
    }))?;
    replay(&output, &operations)?;
    let mut command = CommandSpec::new_os(
        vec![
            resolve_non_strict(binary)?.into_os_string(),
            resolve_non_strict(&output.join("request.jsonl"))?.into_os_string(),
            resolve_non_strict(&output.join("actual.jsonl"))?.into_os_string(),
            resolve_non_strict(&output.join(if matches!(kind, Kind::Reference) {
                "rust"
            } else {
                "native"
            }))?
            .into_os_string(),
        ],
        Duration::MAX,
    );
    command.inherit_output = true;
    command.inherit_stdin = true;
    let child = runner.run(&command)?;
    if child.code != 0 {
        return Err(format!("command-reader candidate exited {}", child.code).into());
    }
    let actual = text(fs::read(output.join("actual.jsonl"))?)?;
    let actual = lines(&actual)
        .into_iter()
        .map(parse)
        .collect::<Result<Vec<_>>>()?;
    if actual.len() != expected.len() {
        return Err("command-reader trace lengths differ".into());
    }
    let differences = expected
        .iter()
        .zip(&actual)
        .enumerate()
        .filter(|(_, (e, a))| !json::equal(e, a))
        .collect::<Vec<_>>();
    match kind {
        Kind::Reference => {
            if !differences.is_empty() {
                return Err("command-reader action, consumed ID or repeat state differs".into());
            }
            let returned = expected
                .iter()
                .filter(|v| {
                    v.get("action")
                        .is_some_and(|v| !matches!(v.view(), JsonView::Null))
                })
                .count();
            let report = format!("{{\"reads\": {}, \"commands_returned\": {returned}, \"comparison\": \"exact action, consumed ID and repeat state\", \"device_validation\": false}}",expected.len());
            let value = JsonValue::parse(&report)?;
            fs::write(output.join("report.json"), json::pretty(&value, 0)? + "\n")?;
            printed.write_all(report.as_bytes())?;
            printed.write_all(b"\n")?;
        }
        Kind::JsonBoundary => {
            // The original hashes current on-disk source and the original binary argument
            // only after the child and strict pairing, even if the child changed either.
            let model = root.join("openpilot/selfdrive/carrot/bluetooth/model.py");
            let source_sha = sha(&fs::read(&model)?);
            let binary_sha = sha(&fs::read(binary)?);
            let rows = differences
                .iter()
                .map(|(index, (e, a))| {
                    Ok(format!(
                        "{{\"index\": {index}, \"expected\": {}, \"actual\": {}}}",
                        e.to_json()?,
                        a.to_json()?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let differences_value = JsonValue::parse(&format!("[{}]", rows.join(", ")))?;
            let source = python_fs::json_string(model.as_os_str())?;
            // Splice the filesystem node's final JSON directly; never lossily decode or
            // reparse surrogateescaped filesystem text into a different string identity.
            let pretty = format!("{{\n  \"cases\": {},\n  \"differences\": {},\n  \"source\": {source},\n  \"source_sha256\": \"{source_sha}\",\n  \"binary_sha256\": \"{binary_sha}\"\n}}\n",expected.len(),json::pretty(&differences_value,1)?);
            fs::write(output.join("result.json"), pretty)?;
            let compact = format!("{{\"cases\": {}, \"differences\": {}, \"source\": {source}, \"source_sha256\": \"{source_sha}\", \"binary_sha256\": \"{binary_sha}\"}}\n",expected.len(),differences_value.to_json()?);
            printed.write_all(compact.as_bytes())?;
            if !differences.is_empty() {
                return Err("command-reader JSON boundary differences".into());
            }
        }
    }
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
            "usage: check-command-{} --binary BINARY --output OUTPUT",
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
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
