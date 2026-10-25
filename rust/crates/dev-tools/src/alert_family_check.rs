//! Fixed source-owned alert checker corpora. Generic Python policy stays live.
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
    sync::OnceLock,
    time::Duration,
};

pub struct Definition {
    pub checker: &'static str,
    pub checker_sha: &'static str,
    pub manifest: &'static str,
    pub packed: &'static [u8],
    pub live: &'static [&'static str],
    pub cases: usize,
    pub timeout: u64,
    pub callbacks: bool,
    pub command: &'static str,
    pub(crate) corpus: OnceLock<std::result::Result<Corpus, String>>,
}
#[derive(Deserialize)]
pub(crate) struct Corpus {
    input: String,
    source: String,
    report_file: String,
    report_line: String,
    #[serde(default)]
    exceptions: String,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus<'a>(root: &Path, d: &'a Definition) -> Result<&'a Corpus> {
    let manifest: serde_json::Value = serde_json::from_str(d.manifest)?;
    if manifest["implementation"] != "CPython"
        || !manifest["python"]
            .as_str()
            .is_some_and(|s| s.starts_with("3.12."))
        || manifest["cases"].as_u64() != Some(d.cases as u64)
        || manifest["sources"][d.checker] != d.checker_sha
    {
        return Err("alert checker original CPython provenance changed".into());
    }
    let sources = manifest["sources"]
        .as_object()
        .ok_or("missing alert source pins")?;
    if sources.len() != d.live.len() + 1 || d.live.iter().any(|name| !sources.contains_key(*name)) {
        return Err("alert checker live source set changed".into());
    }
    for (name, pin) in sources {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == d.checker && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(
                format!("{name} changed; independently recapture original alert checker").into(),
            );
        }
    }
    if d.callbacks {
        let directory = root.join("openpilot/selfdrive/ui/translations");
        let mut actual = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        actual.retain(|path| path.extension().is_some_and(|extension| extension == "po"));
        if actual.len() != d.live.iter().filter(|name| name.ends_with(".po")).count() {
            return Err("alert translation source set changed".into());
        }
    }
    // Only immutable embedded bytes are cached. Live files and the translation
    // set above are revalidated on every invocation, including after a cache hit.
    d.corpus
        .get_or_init(|| decode_corpus(d, &manifest).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| error.clone().into())
}
fn decode_corpus(d: &Definition, manifest: &serde_json::Value) -> Result<Corpus> {
    if manifest["gzip_sha256"] != sha(d.packed) {
        return Err("alert compressed corpus changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing alert corpus size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(d.packed)
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("alert corpus changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.input.lines().count() != d.cases || corpus.source.lines().count() != d.cases {
        return Err("alert corpus cardinality changed".into());
    }
    Ok(corpus)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
// The callback difference file uses json.dumps(indent=2, ensure_ascii=False).
fn pretty_utf8(value: &JsonValue) -> Result<String> {
    enum Part {
        Node(JsonValue, usize),
        Text(String),
    }
    let mut pending = vec![Part::Node(value.clone(), 0)];
    let mut out = String::new();
    while let Some(part) = pending.pop() {
        let (value, depth) = match part {
            Part::Text(text) => {
                out.push_str(&text);
                continue;
            }
            Part::Node(value, depth) => (value, depth),
        };
        if depth > 995 {
            return Err("Python indented JSON nesting limit exceeded".into());
        }
        let (open, close, entries) = match value.view() {
            JsonView::Array(values) => (
                "[",
                "]",
                values.into_iter().map(|v| (None, v)).collect::<Vec<_>>(),
            ),
            JsonView::Object(values) => (
                "{",
                "}",
                values
                    .into_iter()
                    .map(|(k, v)| (Some(k.to_vec()), v))
                    .collect(),
            ),
            _ => {
                out.push_str(&value.to_json_utf8()?);
                continue;
            }
        };
        if depth == 995 {
            return Err("Python indented JSON container nesting limit exceeded".into());
        }
        out.push_str(open);
        if entries.is_empty() {
            out.push_str(close);
            continue;
        }
        pending.push(Part::Text(format!("\n{}{close}", "  ".repeat(depth))));
        for (i, (key, value)) in entries.into_iter().enumerate().rev() {
            pending.push(Part::Node(value, depth + 1));
            let mut prefix = format!(
                "{}\n{}",
                if i == 0 { "" } else { "," },
                "  ".repeat(depth + 1)
            );
            if let Some(key) = key {
                prefix.push_str(
                    &JsonValue::codepoints(key)
                        .ok_or("invalid key")?
                        .to_json_utf8()?,
                );
                prefix.push_str(": ");
            }
            pending.push(Part::Text(prefix));
        }
    }
    Ok(out)
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    d: &Definition,
) -> Result<()> {
    let corpus = corpus(root, d)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    if d.callbacks {
        fs::write(output.join("source-exceptions.json"), &corpus.exceptions)?;
    }
    fs::write(output.join("input.jsonl"), &corpus.input)?;
    let mut argv = vec![resolve_non_strict(binary)?.into_os_string()];
    if d.callbacks {
        argv.push(
            resolve_non_strict(&root.join("openpilot/selfdrive/ui/translations"))?.into_os_string(),
        );
    }
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(d.timeout));
    spec.input = Some(corpus.input.clone().into_bytes());
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let child = runner.run(&spec)?;
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join("native.jsonl"), &stdout)?;
    fs::write(output.join("native.stderr"), &stderr)?;
    // These originals publish source output after child capture, before status validation.
    fs::write(output.join("source.jsonl"), &corpus.source)?;
    if child.code != 0 {
        return Err(format!("alert candidate exited {}", child.code).into());
    }
    let mut lines = stdout
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    // Parse every candidate row before cardinality/comparison failure becomes
    // observable. Equality is pure, so matching rows can be discarded without
    // retaining a second full parsed copy of the manager's 132 MB response.
    // Byte-identical pinned rows were decoded independently by original CPython.
    let source = corpus.source.lines().collect::<Vec<_>>();
    let mut mismatch = false;
    let mut changed = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if source.get(index) == Some(line) {
            continue;
        }
        let actual = json::parse_text(line)?;
        if let Some(expected) = source.get(index) {
            let expected = json::parse_text(expected)?;
            if !json::equal(&expected, &actual) {
                mismatch = true;
                if d.callbacks {
                    changed.push((index, expected, actual));
                }
            }
        }
    }
    if lines.len() != d.cases {
        return Err("alert candidate row count differs".into());
    }
    let mut differences = String::from("[");
    if d.callbacks {
        let requests = corpus.input.lines().collect::<Vec<_>>();
        for (position, (index, expected, actual)) in changed.into_iter().enumerate() {
            if position != 0 {
                differences.push_str(", ");
            }
            let request = requests.get(index).ok_or("missing alert request")?;
            differences.push_str(&format!(
                "{{\"index\":{index},\"request\":{request},\"source\":{},\"native\":{}}}",
                expected.to_json()?,
                actual.to_json()?
            ));
        }
    }
    if d.callbacks {
        differences.push(']');
        let path = output.join("differences.json");
        let formatted = pretty_utf8(&json::parse_text(&differences)?);
        // Python serializes first, then opens the file, then encodes its text.
        // A lone surrogate fails encoding after truncation; JSON depth errors
        // instead occur before opening the file.
        if formatted
            .as_ref()
            .err()
            .is_some_and(|error| error.downcast_ref::<std::fmt::Error>().is_some())
        {
            fs::File::create(&path)?;
        }
        fs::write(path, formatted?)?;
    }
    if mismatch {
        return Err("alert source/native policy output differs".into());
    }
    let binary_sha = sha(&fs::read(binary)?);
    fs::write(
        output.join("manifest.json"),
        corpus.report_file.replace("@@BINARY_SHA256@@", &binary_sha),
    )?;
    printed.write_all(
        corpus
            .report_line
            .replace("@@BINARY_SHA256@@", &binary_sha)
            .as_bytes(),
    )?;
    Ok(())
}
pub fn cli(d: &Definition) -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: {} --binary EXAMPLE --output DIRECTORY", d.command);
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
        d,
    )
}
