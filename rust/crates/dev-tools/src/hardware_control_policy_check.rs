//! Independent fixed hardware policy gate; generic source fixture remains separate.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_hardware_control.py";
const CHECKER_SHA256: &str = "bfefdfa8af3bba6df3ebaff7be31bee399d319ce23b7d7c96c70152f98d90291";
#[derive(Deserialize)]
struct Case {
    name: String,
    input: String,
    expected: String,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corpus(root: &Path) -> Result<Vec<Case>> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/hardware-control-policy/manifest.json"
    ))?;
    let packed = include_bytes!("../tests/fixtures/hardware-control-policy/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("hardware corpus compressed hash mismatch".into());
    }
    let size = manifest["bytes"].as_u64().ok_or("missing corpus size")?;
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("hardware corpus integrity mismatch".into());
    }
    for (name, expected) in manifest["sources"]
        .as_object()
        .ok_or("missing source pins")?
    {
        if name == CHECKER {
            if expected != CHECKER_SHA256 {
                return Err("hardware checker snapshot changed".into());
            }
            // Explicit retired checker identity remains pinned in the corpus.
            match fs::read(root.join(name)) {
                Ok(bytes) if sha(&bytes) != CHECKER_SHA256 => {
                    return Err("reintroduced hardware checker changed".into())
                }
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into())
                }
                _ => {}
            }
        } else if expected != &sha(&fs::read(root.join(name))?) {
            return Err(format!("{name} changed; independently recapture hardware policy").into());
        }
    }
    let rows: Vec<Case> = serde_json::from_slice(&plain)?;
    if manifest["cases"].as_u64() != Some(rows.len() as u64) {
        return Err("hardware corpus count mismatch".into());
    }
    Ok(rows)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    runners: &[OsString],
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let rows = corpus(root)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let mut argv = runners.to_vec();
    argv.push(resolve_non_strict(binary)?.into_os_string());
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::MAX);
    spec.input = Some(
        rows.iter()
            .map(|row| format!("{}\n", row.input))
            .collect::<String>()
            .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let captured = runner.run(&spec)?;
    let stdout = text(captured.stdout)?;
    let stderr = text(captured.stderr)?;
    if captured.code != 0 {
        return Err(format!("hardware candidate exited {}", captured.code).into());
    }
    fs::write(output.join("native.stdout"), &stdout)?;
    fs::write(
        output.join("native.stderr"),
        if stderr.is_empty() {
            "<empty>\n"
        } else {
            &stderr
        },
    )?;
    let mut lines = stdout
        .split([
            '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .collect::<Vec<_>>();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let results = lines
        .into_iter()
        .map(json::parse_text)
        .collect::<Result<Vec<_>>>()?;
    if results.len() != rows.len() {
        return Err("hardware candidate output count mismatch".into());
    }
    for (row, actual) in rows.iter().zip(results) {
        let expected = json::parse_text(&row.expected)?;
        let report = json::parse_text(&format!(
            "{{\"input\":{},\"source\":{},\"native\":{}}}",
            row.input,
            row.expected,
            actual.to_json()?
        ))?;
        fs::write(
            output.join(format!("{}.json", row.name)),
            json::pretty_with_limit(&report, 0, 996)?,
        )?;
        if !json::equal(&expected, &actual) {
            return Err(format!("hardware mismatch: {}", row.name).into());
        }
        writeln!(printed, "{} PASS", row.name)?;
    }
    let args = argv
        .iter()
        .map(|arg| python_fs::json_string(arg))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let report = json::parse_text(&format!(
        "{{\"result\":\"PASS\",\"cases\":{},\"argv\":[{args}]}}",
        rows.len()
    ))?;
    fs::write(output.join("result.json"), json::pretty(&report, 0)?)?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let options = scan_os_with_positionals(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[OptionSpec::append("--runner")],
        2,
    )?;
    if options.help {
        println!("Usage: check-hardware-control-policy BINARY OUTPUT [--runner ARG]...");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    check(
        &root,
        Path::new(&options.positionals[0]),
        Path::new(&options.positionals[1]),
        options
            .append_values
            .get("--runner")
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
