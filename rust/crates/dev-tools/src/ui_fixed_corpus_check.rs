//! Complete, independently captured original fixed-input UI parity corpora.
//! Capture evidence is Linux x86_64; these gates do not establish ARM parity.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Qr,
    Torque,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(kind: Kind) -> Result<(Vec<u8>, Vec<u8>)> {
    let (input, source, row) = match kind {
        Kind::Qr => (
            include_bytes!("../tests/fixtures/ui-fixed-corpus/qr-input.json.gz").as_slice(),
            include_bytes!("../tests/fixtures/ui-fixed-corpus/qr-source.json.gz").as_slice(),
            0,
        ),
        Kind::Torque => (
            include_bytes!("../tests/fixtures/ui-fixed-corpus/torque-input.json.gz").as_slice(),
            include_bytes!("../tests/fixtures/ui-fixed-corpus/torque-source.json.gz").as_slice(),
            1,
        ),
    };
    let compressed: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-fixed-corpus/gzip-sha256.json"
    ))?;
    let prefix = if row == 0 { "qr" } else { "torque" };
    for (name, bytes) in [("input", input), ("source", source)] {
        if compressed[format!("{prefix}-{name}.json.gz")].as_str() != Some(&digest(bytes)) {
            return Err("compressed corpus integrity mismatch".into());
        }
    }
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-fixed-corpus/corpus.json"
    ))?;
    let inflate = |bytes: &[u8], name: &str| -> Result<Vec<u8>> {
        let mut out = vec![];
        flate2::read::GzDecoder::new(bytes)
            .take(
                manifest[row][format!("{name}_bytes")]
                    .as_u64()
                    .ok_or("missing size")?
                    + 1,
            )
            .read_to_end(&mut out)?;
        if digest(&out)
            != manifest[row][format!("{name}_sha256")]
                .as_str()
                .ok_or("missing digest")?
        {
            return Err("fixed corpus integrity mismatch".into());
        }
        Ok(out)
    };
    Ok((inflate(input, "input")?, inflate(source, "source")?))
}
fn pin(root: &Path, kind: Kind) -> Result<()> {
    let locked: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let (name, expected) = match kind {
        Kind::Qr => (
            "qrcode",
            include_str!("../tests/fixtures/ui-fixed-corpus/qrcode-lock.toml"),
        ),
        Kind::Torque => (
            "numpy",
            include_str!("../tests/fixtures/ui-fixed-corpus/numpy-lock.toml"),
        ),
    };
    let expected: toml::Value = toml::from_str(expected)?;
    let entries = locked
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing locked packages")?;
    let found = entries
        .iter()
        .filter(|v| v.get("name").and_then(toml::Value::as_str) == Some(name))
        .collect::<Vec<_>>();
    if found.len() != 1 || *found[0] != expected {
        return Err(
            format!("{name} source pin changed; independently recapture original corpus").into(),
        );
    }
    if matches!(kind, Kind::Torque) {
        for (path, hash) in [
            (
                "openpilot/selfdrive/ui/mici/onroad/torque_bar.py",
                "0d5d5d6ed57438c9c0418d3fcc08431e938100d88e8740b5f3449bccf1a7749d",
            ),
            (
                "rust/tools/ui_application_qa/torque_geometry_source.py",
                "4fa0b30a027e9a9e9ef7ff2b9905b1595b846c005112a9bc48dc67ebcc68e750",
            ),
        ] {
            if digest(&fs::read(root.join(path))?) != hash {
                return Err(format!(
                    "{path} source pin changed; independently recapture original corpus"
                )
                .into());
            }
        }
    }
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
// Independently captured original whole-script admission: depth9997 succeeds;
// depth9998 fails before Torque source/native writes. A text BOM remains invalid.
fn parse_text(text: &str) -> Result<JsonValue> {
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
                depth = depth.saturating_sub(1)
            }
        }
    }
    Ok(JsonValue::parse(text)?)
}
/// Original Path construction normalizes separators/dot without resolving symlinks.
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::{ffi::OsStrExt, ffi::OsStringExt};
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
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/')
        }
        out.extend(part)
    }
    if out.is_empty() {
        out.push(b'.')
    }
    std::ffi::OsString::from_vec(out).into()
}
/// Check the candidate; write artifacts at the same original execution boundaries.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
) -> Result<String> {
    pin(root, kind)?;
    let (input, source) = corpus(kind)?;
    fs::create_dir_all(output)?;
    fs::write(output.join("input.json"), &input)?;
    let mut spec = CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
    spec.input = Some(input);
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = runner.run(&spec)?;
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("candidate exited {}", captured.code).into());
    }
    if matches!(kind, Kind::Qr) {
        fs::write(output.join("native.json"), &stdout)?;
    }
    fs::write(
        output.join("native.stderr"),
        if stderr.is_empty() {
            "(no stderr)\n"
        } else {
            &stderr
        },
    )?;
    if matches!(kind, Kind::Qr) {
        fs::write(output.join("source.json"), &source)?;
    }
    let native = parse_text(&stdout)?;
    if matches!(kind, Kind::Torque) {
        fs::write(output.join("source.json"), &source)?;
        fs::write(output.join("native.json"), &stdout)?;
    }
    let expected = JsonValue::parse(std::str::from_utf8(&source)?)?;
    let expected = json::sequence(&expected)?;
    let actual = json::sequence(&native)?;
    if expected.len() != actual.len() {
        return Err("original/native length mismatch".into());
    }
    let mut differences = vec![];
    for (i, (a, b)) in expected.iter().zip(&actual).enumerate() {
        if !json::equal(a, b) {
            if matches!(kind, Kind::Qr) {
                return Err(format!("QR mismatch at case {i}").into());
            }
            differences.push(format!(
                "    {{\n      \"step\": {i},\n      \"source\": {},\n      \"native\": {}\n    }}",
                json::pretty_with_limit(a, 3, 996)?,
                json::pretty_with_limit(b, 3, 996)?
            ));
        }
    }
    let exact = differences.is_empty();
    let result=match kind{Kind::Qr=>"{\n  \"cases\": 64,\n  \"exact\": true,\n  \"correction\": [\n    \"L\",\n    \"M\"\n  ],\n  \"source_dependency\": \"uv.lock qrcode 8.2\"\n}".to_owned(),Kind::Torque=>format!("{{\n  \"cases\": 1603,\n  \"exact_f32_points\": {exact},\n  \"differences\": {}\n}}",if exact{"[]".to_owned()}else{format!("[\n{}\n  ]",differences.join(",\n"))})};
    fs::write(output.join("result.json"), result)?;
    if !exact {
        return Err("torque geometry mismatch".into());
    }
    Ok(match kind{Kind::Qr=>"PASS: 64 source/native QR segmentation, mask selection and complete module matrices\n",Kind::Torque=>"PASS 1603 original/native torque arcs, exact float32 points with quantized hits, ties and LRU eviction\n"}.into())
}
pub fn cli(kind: Kind) -> Result<()> {
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!(
            "usage: {} --binary BINARY --output OUTPUT",
            if matches!(kind, Kind::Qr) {
                "check-ui-qr"
            } else {
                "check-ui-torque-geometry"
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
            Path::new(&parsed.values["--binary"]),
            Path::new(&parsed.values["--output"]),
            kind,
            &mut NativeRunner
        )?
    );
    Ok(())
}
