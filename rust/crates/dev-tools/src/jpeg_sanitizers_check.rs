//! Full-codec ASan/UBSan ownership validation, including build and failure receipts.
use crate::{
    disk_space,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
pub const FLAGS: &str = "-fsanitize=address,undefined -fno-omit-frame-pointer";
pub const SUCCESS: &str = "PASS: instrumented complete libjpeg-turbo codec,C/CXX owned boundary,ASan+UBSan+leak detection,invalid dimensions and returned bytes ownership";
fn text(bytes: Vec<u8>) -> Result<String> {
    // subprocess text=True uses universal-newline translation after strict UTF-8.
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
pub fn check(
    rust: &Path,
    target: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    free: u64,
) -> Result<()> {
    fs::create_dir_all(output)?;
    disk_space::Space::new(free, 1024 * 1024 * 1024)?.require()?;
    let target_absolute = resolve_non_strict(target)?;
    let mut environment = BTreeMap::from([
        (
            "CARGO_TARGET_DIR".into(),
            target_absolute.to_string_lossy().into_owned(),
        ),
        ("CARGO_INCREMENTAL".into(), "0".into()),
        ("CFLAGS".into(), FLAGS.into()),
        ("CXXFLAGS".into(), FLAGS.into()),
        (
            "RUSTFLAGS".into(),
            "-C link-arg=-lasan -C link-arg=-lubsan".into(),
        ),
    ]);
    let mut build = CommandSpec::new(
        [
            "cargo",
            "test",
            "-p",
            "openpilot-jpeg",
            "--test",
            "ownership",
            "--no-run",
            "--locked",
            "--message-format=json",
            "-j2",
        ]
        .map(str::to_owned)
        .into(),
        Duration::from_secs(1200),
    );
    build.cwd = Some(rust.into());
    build.env = environment.clone();
    let built = runner.run(&build)?;
    let build_stdout = text(built.stdout)?;
    let build_stderr = text(built.stderr)?;
    fs::write(output.join("build.jsonl"), &build_stdout)?;
    fs::write(output.join("build.log"), &build_stderr)?;
    if built.code != 0 {
        return Err(format!("sanitizer build failed: {build_stderr}").into());
    }
    fn truthy(value: &Value) -> bool {
        match value {
            Value::Null => false,
            Value::Bool(value) => *value,
            Value::Number(value) => value.as_f64() != Some(0.0),
            Value::String(value) => !value.is_empty(),
            Value::Array(value) => !value.is_empty(),
            Value::Object(value) => !value.is_empty(),
        }
    }
    let mut binaries = vec![];
    // Match Python str.splitlines(), including Unicode separators. Text-mode
    // CR and CRLF have already been normalized by `text`.
    for line in build_stdout.split_terminator([
        '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
    ]) {
        let row: Value = serde_json::from_str(line)?;
        let object = row.as_object().ok_or("Cargo record must be an object")?;
        if object.get("reason").and_then(Value::as_str) == Some("compiler-artifact") {
            if let Some(binary) = object.get("executable").filter(|value| truthy(value)) {
                binaries.push(binary.clone());
            }
        }
    }
    if binaries.len() != 1 {
        return Err("expected exactly one sanitizer ownership executable".into());
    }
    let compiler = CommandSpec::new(
        vec!["g++".into(), "-print-file-name=libasan.so".into()],
        Duration::from_secs(30),
    );
    let library = runner.run(&compiler)?;
    // check_output inherits compiler stderr; preserve that effect without merging it.
    {
        use std::io::Write;
        std::io::stderr().write_all(&library.stderr)?;
    }
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
    let mut run = CommandSpec::new(
        vec![
            binaries[0]
                .as_str()
                .ok_or("Cargo executable must be a path string")?
                .into(),
            "--nocapture".into(),
        ],
        Duration::from_secs(60),
    );
    run.env = environment;
    let result = runner.run(&run)?;
    let stdout = text(result.stdout)?;
    let stderr = text(result.stderr)?;
    fs::write(output.join("run.log"), stdout + &stderr)?;
    if result.code != 0 {
        return Err(format!("sanitizer ownership test failed: {stderr}").into());
    }
    let mut configs = vec![];
    match fs::read_dir(target.join("debug/build")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("openpilot-jpeg-")
                {
                    let cache = entry.path().join("out/jpeg/CMakeCache.txt");
                    if fs::symlink_metadata(&cache).is_ok() {
                        configs.push(cache);
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if configs.len() != 1
        || !fs::read_to_string(&configs[0])?.contains(&format!("CMAKE_C_FLAGS:STRING={FLAGS}"))
    {
        return Err("expected exactly one complete codec cache with sanitizer C flags".into());
    }
    let report = json!({"pass":true,"binary":binaries[0],"full_codec_c_flags":FLAGS,"returncode":result.code});
    fs::write(
        output.join("result.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!("{SUCCESS}");
    Ok(())
}
pub use crate::argparse_scan::ArgumentError;
fn parse_arguments(
    args: &[String],
) -> std::result::Result<crate::argparse_scan::Parsed, ArgumentError> {
    use crate::argparse_scan::{scan, OptionSpec as S};
    scan(
        args,
        &[S::value("--target-dir", true), S::value("--output", true)],
    )
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_arguments(&args)?;
    if parsed.help {
        println!("Usage: check-jpeg-sanitizers --target-dir TARGET --output OUTPUT");
        return Ok(());
    }
    let target = PathBuf::from(&parsed.values["--target-dir"]);
    let output = PathBuf::from(&parsed.values["--output"]);
    // Original tool inspects its Rust workspace, then builds into the supplied target.
    let rust = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    fs::create_dir_all(&output)?;
    let free = disk_space::inspect(&rust, 1024 * 1024 * 1024)?.free_bytes;
    check(&rust, &target, &output, &mut NativeRunner, free)
}

#[cfg(test)]
mod original_cli_interfaces {
    #[test]
    fn original_argparse_admission() {
        let rows: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/panda-cli/five-interfaces.json"
        ))
        .unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| ["jpeg"].contains(&row["family"].as_str().unwrap()))
        {
            let args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let result = super::parse_arguments(&args);
            assert_eq!(result.is_ok(), row["original_exit"] == 0, "{row}");
            if let Ok(parsed) = result {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{row}");
                if !parsed.help {
                    for (key, value) in &parsed.values {
                        let name = key.trim_start_matches("--").replace('-', "_");
                        let expected = &row["parsed"][&name];
                        if expected.is_boolean() {
                            assert_eq!(expected, true, "{row}");
                        } else {
                            assert_eq!(value, expected.as_str().unwrap(), "{row}");
                        }
                    }
                    if row["family"] == "bootlog" {
                        assert_eq!(
                            parsed.positionals[0],
                            row["parsed"]["output"].as_str().unwrap(),
                            "{row}"
                        );
                    }
                }
            }
        }
    }
}
