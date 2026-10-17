//! Live original C++ Params/std::stoi differential for the fixed beep inputs.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    params_reference_check::{build_native, ParamsRunner},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const CHECKER: &str = "rust/tools/check_beepd_integer.py";
const CHECKER_SHA: &str = "0940c83ae162ca1fb97f49b391f58637a1d0f27e14af8b23f7aeee3fff8d883f";
const HELPER: &str = "rust/tools/beep_integer_reference.cc";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Deserialize)]
struct Case {
    bytes: Vec<u8>,
}
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
fn manifest() -> Result<serde_json::Value> {
    Ok(serde_json::from_str(include_str!(
        "../tests/fixtures/beep-integer/manifest.json"
    ))?)
}
fn validate_source(root: &Path) -> Result<serde_json::Value> {
    let manifest = manifest()?;
    if manifest["sources"][CHECKER] != CHECKER_SHA
        || manifest["implementation"] != "CPython"
        || manifest["cases"] != 278
    {
        return Err("beep integer original provenance changed".into());
    }
    for (name, pin) in manifest["sources"]
        .as_object()
        .ok_or("missing beep integer source pins")?
    {
        let bytes = match fs::read(root.join(name)) {
            Err(error) if name == CHECKER && error.kind() == std::io::ErrorKind::NotFound => {
                continue
            }
            result => result?,
        };
        if pin.as_str() != Some(&sha(&bytes)) {
            return Err(
                format!("{name} changed; independently recapture beep integer reference").into(),
            );
        }
    }
    Ok(manifest)
}
fn cases(root: &Path) -> Result<Vec<Case>> {
    let manifest = validate_source(root)?;
    let packed = include_bytes!("../tests/fixtures/beep-integer/corpus.json.gz");
    if manifest["gzip_sha256"] != sha(packed) {
        return Err("beep integer compressed inputs changed".into());
    }
    let size = manifest["bytes"]
        .as_u64()
        .ok_or("missing beep integer input size")?;
    let mut plain = vec![];
    flate2::read::GzDecoder::new(packed.as_slice())
        .take(size + 1)
        .read_to_end(&mut plain)?;
    if plain.len() as u64 != size || manifest["sha256"] != sha(&plain) {
        return Err("beep integer inputs changed".into());
    }
    let corpus: Corpus = serde_json::from_slice(&plain)?;
    if corpus.cases.len() != 278 {
        return Err("beep integer input count changed".into());
    }
    Ok(corpus.cases)
}
fn reference_receipt(reference: &Path) -> PathBuf {
    reference.with_extension("provenance.json")
}
fn validate_reference(root: &Path, reference: &Path) -> Result<()> {
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(reference_receipt(reference))?)?;
    if receipt["kind"] != "beep-integer-original-params"
        || receipt["binary_sha256"] != sha(&fs::read(reference)?)
    {
        return Err("beep integer source reference identity changed".into());
    }
    for (name, pin) in receipt["sources"]
        .as_object()
        .ok_or("missing native reference source receipt")?
    {
        if pin.as_str() != Some(&sha(&fs::read(root.join(name))?)) {
            return Err(format!("native integer reference source changed: {name}").into());
        }
    }
    for name in [
        HELPER,
        "openpilot/common/params.cc",
        "openpilot/common/params.h",
        "openpilot/common/util.cc",
    ] {
        if receipt["sources"][name].is_null() {
            return Err("native integer reference lacks original sources".into());
        }
    }
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn with_store<T>(action: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let temporary = tempfile::Builder::new().prefix("beep-number-").tempdir()?;
    let result = action(temporary.path());
    let owned = temporary.keep();
    crate::params_reference_check::cleanup_temporary(&owned)?;
    result
}
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let cases = cases(root)?;
    fs::create_dir_all(output)?;
    let reference = resolve_non_strict(reference)?;
    validate_reference(root, &reference)?;
    let mut rows = vec![];
    for case in cases {
        let mut pair = vec![];
        let mut outcomes = vec![];
        let mut diagnostics = vec![];
        for implementation in ["source-reference", "rust"] {
            let (row, outcome, stderr) = with_store(|directory| {
                let prefix = "beep_integer";
                fs::create_dir(directory.join(prefix))?;
                fs::write(
                    directory.join(prefix).join("SoundVolumeAdjust"),
                    &case.bytes,
                )?;
                fs::create_dir(directory.join("bin"))?;
                let mut spec = if implementation == "source-reference" {
                    CommandSpec::new_os(
                        vec![
                            reference.as_os_str().to_owned(),
                            directory.as_os_str().to_owned(),
                        ],
                        Duration::from_secs(5),
                    )
                } else {
                    let mut spec = CommandSpec::new_os(
                        vec![resolve_non_strict(binary)?.into_os_string()],
                        Duration::from_secs(5),
                    );
                    let config = serde_json::json!({"root":directory,"binding":reference,"trace":directory.join("trace.jsonl"),"mode":"integer","actions":[],"bytes":case.bytes});
                    spec.input = Some((serde_json::to_string(&config)? + "\n").into_bytes());
                    spec
                };
                spec.set_os_env(BTreeMap::from([
                    (
                        OsString::from("PARAMS_ROOT"),
                        directory.as_os_str().to_owned(),
                    ),
                    (OsString::from("OPENPILOT_PREFIX"), prefix.into()),
                    (
                        OsString::from("BEEP_FIXTURE_ROOT"),
                        directory.as_os_str().to_owned(),
                    ),
                    (
                        OsString::from("PATH"),
                        directory.join("bin").into_os_string(),
                    ),
                    (OsString::from("PYTHONUNBUFFERED"), "1".into()),
                ]));
                spec.stdout_limit = usize::MAX;
                spec.stderr_limit = usize::MAX;
                let result = runner.run(&spec)?;
                let stdout = text(result.stdout)?;
                let stderr = text(result.stderr)?;
                let outcome = if implementation == "source-reference" && result.code == -6 {
                    if !stderr.contains("stoi") {
                        return Err("original integer abort lacks stoi diagnostic".into());
                    }
                    json::parse_text(if stderr.contains("out_of_range") {
                        r#"{"error":"Range"}"#
                    } else {
                        r#"{"error":"Invalid"}"#
                    })?
                } else {
                    if result.code != 0 {
                        return Err(format!(
                            "beep integer {implementation} exited {}",
                            result.code
                        )
                        .into());
                    }
                    json::parse_text(&stdout)?
                };
                let row = format!("{{\"implementation\":\"{implementation}\",\"exit_code\":{},\"outcome\":{},\"stderr\":{}}}",result.code,outcome.to_json()?,JsonValue::text(&stderr).to_json()?);
                Ok((row, outcome, stderr))
            })?;
            pair.push(row);
            outcomes.push(outcome);
            diagnostics.push(stderr);
        }
        if !json::equal(&outcomes[0], &outcomes[1]) {
            return Err("beep original/native integer outcomes differ".into());
        }
        if diagnostics
            .iter()
            .any(|stderr| stderr.contains("Failed to cast param"))
        {
            return Err("beep integer unexpectedly logged a cast failure".into());
        }
        let hex = case
            .bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        rows.push(format!(
            "{{\"bytes_hex\":\"{hex}\",\"results\":[{}]}}",
            pair.join(",")
        ));
    }
    let report = json::parse_text(&format!(
        "{{\"passed\":true,\"cases\":{},\"results\":[{}]}}",
        rows.len(),
        rows.join(",")
    ))?;
    fs::write(
        output.join("summary.json"),
        json::pretty_with_limit(&report, 0, 995)? + "\n",
    )?;
    writeln!(
        printed,
        "PASS: {} original C++ Params/native integer cases, including all leading bytes and fatal ranges",
        rows.len()
    )?;
    Ok(())
}
/// Reuse the established native Params source build for schema staging; compile
/// this narrow getInt entrypoint against the same unchanged Params/util files.
pub fn build_reference(
    root: &Path,
    output: &Path,
    prefix: Option<&Path>,
    runner: &mut dyn ParamsRunner,
) -> Result<PathBuf> {
    validate_source(root)?;
    // A source build owns a fresh directory. Never reuse an earlier executable
    // or receipt after a failed attempt, or follow caller-owned receipt links.
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    openpilot_disk_space::inspect(output, 512 * 1024 * 1024)?.require()?;
    build_native(root, output, prefix, runner)?;
    let binary = output.join("beep-integer-reference");
    let mut argv = vec![
        OsString::from("g++"),
        "-std=c++17".into(),
        "-pthread".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wl,--gc-sections".into(),
    ];
    for path in [
        root.join("openpilot"),
        output.to_path_buf(),
        output.join("cereal/gen/cpp"),
    ] {
        argv.push("-I".into());
        argv.push(path.into_os_string());
    }
    if let Some(prefix) = prefix {
        argv.push("-I".into());
        argv.push(prefix.join("include").into_os_string());
    }
    for name in [
        "openpilot/common/params.cc",
        "openpilot/common/util.cc",
        HELPER,
    ] {
        argv.push(root.join(name).into_os_string());
    }
    argv.extend([OsString::from("-o"), binary.as_os_str().to_owned()]);
    let mut spec = CommandSpec::new_os(argv, Duration::MAX);
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    if runner.run(&spec)?.code != 0 {
        return Err("beep integer original source compilation failed".into());
    }
    let source_names = [
        HELPER,
        "openpilot/common/params.cc",
        "openpilot/common/params.h",
        "openpilot/common/params_keys.h",
        "openpilot/common/util.cc",
        "openpilot/common/util.h",
        "openpilot/common/queue.h",
        "openpilot/common/swaglog.h",
        "openpilot/system/hardware/hw.h",
        "openpilot/system/hardware/base.h",
        "openpilot/system/hardware/pc/hardware.h",
    ];
    let mut sources = BTreeMap::new();
    for name in source_names {
        sources.insert(name, sha(&fs::read(root.join(name))?));
    }
    let receipt = serde_json::json!({"kind":"beep-integer-original-params","binary_sha256":sha(&fs::read(&binary)?),"sources":sources,"source_changes":"None. Narrow entrypoint calls original Params::getInt without exception interception."});
    fs::write(
        reference_receipt(&binary),
        serde_json::to_string_pretty(&receipt)? + "\n",
    )?;
    Ok(binary)
}
struct BuildRunner;
impl Runner for BuildRunner {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        NativeRunner.run(spec)
    }
}
impl ParamsRunner for BuildRunner {
    fn locked_put(&mut self, _writer: &Path, _store: &Path) -> Result<()> {
        Err("integer builder has no lock scenario".into())
    }
}
fn root() -> PathBuf {
    std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--reference", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-beep-integer --binary EXAMPLE --reference NATIVE_PARAMS_REFERENCE --output DIRECTORY");
        return Ok(());
    }
    check(
        &root(),
        Path::new(&args.values["--binary"]),
        Path::new(&args.values["--reference"]),
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
pub fn build_cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--capnp-prefix", false),
        ],
    )?;
    if args.help {
        println!("usage: build-beep-integer-reference --output DIRECTORY [--capnp-prefix PREFIX]");
        return Ok(());
    }
    let root = resolve_non_strict(&root())?;
    let output = resolve_non_strict(Path::new(&args.values["--output"]))?;
    let binary = build_reference(
        &root,
        &output,
        args.values.get("--capnp-prefix").map(Path::new),
        &mut BuildRunner,
    )?;
    println!("{}", binary.display());
    Ok(())
}
