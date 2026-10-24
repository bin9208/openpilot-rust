//! Fixed original plot-sample corpus checker; generic behavior lives separately.
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
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_ui_plot_samples.py";
const HELPER: &str = "rust/tools/ui_application_qa/plot_samples_source.py";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn validate_source(root: &Path) -> Result<()> {
    if sha(include_bytes!(
        "../tests/fixtures/plot-samples/manifest.json"
    )) != "dd907c6d80cb211a118ab700673bda441233f97fcf8b75263d140ae3908f640e"
    {
        return Err("plot provenance manifest changed".into());
    }
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/plot-samples/manifest.json"))?;
    for (path, hash) in manifest["sources"]
        .as_object()
        .ok_or("plot source pins missing")?
    {
        match fs::read(root.join(path)) {
            Ok(bytes) if Some(sha(&bytes).as_str()) != hash.as_str() => {
                return Err("plot source changed; independently recapture".into())
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && (path == CHECKER || path == HELPER) => {}
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}
fn corpus(root: &Path) -> Result<serde_json::Value> {
    validate_source(root)?;
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/plot-samples/manifest.json"))?;
    let packed = include_bytes!("../tests/fixtures/plot-samples/original.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("plot compressed corpus changed".into());
    }
    let size = manifest["bytes"].as_u64().ok_or("plot size missing")?;
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size || manifest["sha256"] != sha(&bytes) {
        return Err("plot corpus changed".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let b = path.as_os_str().as_bytes();
    let mut out = if b.starts_with(b"//") && !b.starts_with(b"///") {
        b"//".to_vec()
    } else if b.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in b
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    std::ffi::OsString::from_vec(out).into()
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let corpus = corpus(root)?;
    let fixed = &corpus["fixed"];
    let input = fixed["input"].as_str().ok_or("plot input missing")?;
    let source = fixed["source"].as_str().ok_or("plot source missing")?;
    fs::write(output.join("input.json"), input)?;
    let mut command = CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
    command.input = Some(input.as_bytes().to_vec());
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let result = runner.run(&command)?;
    // text=True decodes both streams before check=True raises a child failure.
    let stdout = String::from_utf8(result.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let stderr = String::from_utf8(result.stderr)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if result.code != 0 {
        return Err(format!("plot candidate exited {}", result.code).into());
    }
    let native = json::parse_text(&stdout)?;
    fs::write(output.join("source.json"), source)?;
    fs::write(output.join("native.json"), &stdout)?;
    fs::write(
        output.join("native.stderr"),
        if stderr.is_empty() {
            "(no stderr)\n"
        } else {
            &stderr
        },
    )?;
    let expected = json::sequence(&json::parse_text(source)?)?;
    let actual = json::sequence(&native)?;
    // Original zip(strict=True) fails before writing result.json.
    if expected.len() != actual.len() {
        return Err("plot sample output cardinality differs".into());
    }
    let mut differences = vec![];
    for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
        if !json::equal(a, b) {
            differences.push(format!(
                "{{\"step\": {index}, \"source\": {}, \"native\": {}}}",
                a.to_json()?,
                b.to_json()?
            ));
        }
    }
    let report = JsonValue::parse(&format!(
        "{{\"cases\": 5000, \"history_cases\": 53, \"differences\": [{}]}}",
        differences.join(", ")
    ))?;
    fs::write(
        output.join("result.json"),
        json::pretty_with_limit(&report, 0, 996)?,
    )?;
    if !differences.is_empty() {
        return Err("original/native plot sample differences".into());
    }
    printed.write_all(
        fixed["stdout"]
            .as_str()
            .ok_or("plot success message missing")?
            .as_bytes(),
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let options = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if options.help {
        println!("usage: check-ui-plot-samples --binary BINARY --output PATH");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&options.values["--binary"]),
        Path::new(&options.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
