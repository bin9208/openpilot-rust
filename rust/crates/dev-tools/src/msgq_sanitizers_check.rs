//! Instrument the real msgq native boundary and execute its four test artifacts.
use crate::{
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
const FLAGS: &str = "-fsanitize=address,undefined -fno-omit-frame-pointer";
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value.bytes().any(|b| b != b'-' && b != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
pub fn check(rust: &Path, target: &Path, runner: &mut dyn Runner, free: u64) -> Result<()> {
    disk_space::Space::new(free, 4 * 1024 * 1024 * 1024)?.require()?;
    let target = resolve_non_strict(target)?;
    let mut environment = BTreeMap::from([
        (
            "CARGO_TARGET_DIR".into(),
            target.to_string_lossy().into_owned(),
        ),
        ("CXXFLAGS".into(), FLAGS.into()),
        (
            "RUSTFLAGS".into(),
            "-C link-arg=-lasan -C link-arg=-lubsan".into(),
        ),
        ("CARGO_INCREMENTAL".into(), "0".into()),
        ("CARGO_BUILD_JOBS".into(), "2".into()),
    ]);
    let mut build = CommandSpec::new(
        [
            "cargo",
            "test",
            "-p",
            "openpilot-msgq",
            "--test",
            "transport",
            "--test",
            "vision",
            "--test",
            "queued",
            "--test",
            "creation",
            "--no-run",
            "--locked",
            "--message-format=json",
        ]
        .map(str::to_owned)
        .into(),
        Duration::from_secs(1200),
    );
    build.cwd = Some(rust.into());
    build.env = environment.clone();
    build.inherit_stderr = true;
    build.inherit_stdin = true;
    // The original PIPE is unbounded; do not impose the general runner's 16MiB
    // diagnostic cap on the Cargo JSON stream.
    build.stdout_limit = usize::MAX;
    let result = runner.run(&build)?;
    let stdout = text(result.stdout)?;
    if result.code != 0 {
        return Err("msgq sanitizer build failed".into());
    }
    let mut binaries = vec![];
    for line in stdout.split_terminator([
        '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
    ]) {
        let row = JsonValue::parse(line)?;
        if !row.is_object() {
            return Err("Cargo record does not support object lookup".into());
        }
        if row
            .get("reason")
            .is_some_and(|v| v.text_eq("compiler-artifact"))
        {
            if let Some(binary) = row.get("executable").filter(truthy) {
                binaries.push(binary);
            }
        }
    }
    if binaries.len() != 4 {
        return Err(format!(
            "expected creation, transport, queued and VisionIPC test executables, got {}",
            binaries.len()
        )
        .into());
    }
    let mut lookup = CommandSpec::new(
        vec!["g++".into(), "-print-file-name=libasan.so".into()],
        Duration::from_secs(30),
    );
    lookup.inherit_stderr = true;
    lookup.inherit_stdin = true;
    lookup.stdout_limit = usize::MAX;
    let library = runner.run(&lookup)?;
    let library_stdout = text(library.stdout)?;
    if library.code != 0 {
        return Err("libasan lookup failed".into());
    }
    environment.extend([
        (
            "LD_PRELOAD".into(),
            library_stdout
                .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
                .into(),
        ),
        ("ASAN_OPTIONS".into(), "detect_leaks=1".into()),
        ("UBSAN_OPTIONS".into(), "halt_on_error=1".into()),
    ]);
    for binary in binaries {
        let binary = binary
            .to_utf8()
            .ok_or("Cargo executable must be a Unicode path string")?;
        let mut run = CommandSpec::new(vec![binary, "--nocapture".into()], Duration::from_secs(60));
        run.env = environment.clone();
        run.inherit_output = true;
        run.inherit_stdin = true;
        if runner.run(&run)?.code != 0 {
            return Err("msgq sanitizer test failed".into());
        }
    }
    Ok(())
}
pub use crate::argparse_scan::ArgumentError;
fn parse_arguments(
    args: &[String],
) -> std::result::Result<crate::argparse_scan::Parsed, ArgumentError> {
    crate::argparse_scan::scan(
        args,
        &[crate::argparse_scan::OptionSpec::value(
            "--target-dir",
            true,
        )],
    )
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_arguments(&args)?;
    if parsed.help {
        println!("Usage: check-msgq-sanitizers --target-dir TARGET");
        return Ok(());
    }
    let target = PathBuf::from(&parsed.values["--target-dir"]);
    let rust = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    // Inspect the nearest existing target ancestor: the owned cache can be on
    // another filesystem than the source checkout.
    let resolved = resolve_non_strict(&target)?;
    let mut existing = resolved.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or("target has no existing ancestor")?;
    }
    let free = disk_space::inspect(existing, 4 * 1024 * 1024 * 1024)?.free_bytes;
    check(&rust, &resolved, &mut NativeRunner, free)
}

#[cfg(test)]
mod original_cli_interfaces {
    #[test]
    fn original_argparse_admission() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/four-interfaces.json"
        ))
        .unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["family"] == "msgq")
        {
            let args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let result = super::parse_arguments(&args);
            assert_eq!(result.is_ok(), row["exit"] == 0, "{row}");
            if let Ok(parsed) = result {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{row}");
                if !parsed.help {
                    assert_eq!(
                        parsed.values["--target-dir"],
                        row["parsed"]["target_dir"].as_str().unwrap(),
                        "{row}"
                    );
                }
            }
        }
    }
}
