//! Native source/candidate identifier parsing and Params I/O boundary checker.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Debug)]
pub struct Case {
    pub key: String,
    pub value: Vec<u8>,
    pub failure: String,
}
pub fn cases() -> Vec<Case> {
    let mut result = vec![];
    for key in ["RouteCount", "BootCount"] {
        for value in [
            b"".as_slice(),
            b"0",
            b" \t+42tail",
            b" -1suffix",
            b"4294967295",
            b"4294967296",
            b"18446744073709551615",
            b"18446744073709551616",
            b"junk",
            b"23\0tail",
        ] {
            for failure in ["ordinary", "directory", "unreadable", "unwritable"] {
                result.push(Case {
                    key: key.into(),
                    value: value.to_vec(),
                    failure: failure.into(),
                });
            }
        }
    }
    result
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn mode(path: &Path, value: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
fn log(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(
        path,
        if bytes.is_empty() {
            b"<empty>\n"
        } else {
            bytes
        },
    )?;
    Ok(())
}
pub fn run_case(
    binary: &Path,
    case: &Case,
    directory: &Path,
    prefix: &[String],
    runner: &mut dyn Runner,
) -> Result<Value> {
    run_case_os(
        binary,
        case,
        directory,
        &prefix.iter().map(OsString::from).collect::<Vec<_>>(),
        runner,
    )
}
pub fn run_case_os(
    binary: &Path,
    case: &Case,
    directory: &Path,
    prefix: &[OsString],
    runner: &mut dyn Runner,
) -> Result<Value> {
    fs::create_dir_all(directory.parent().ok_or("identifier case parent missing")?)?;
    fs::create_dir(directory)?;
    let params = directory.join("params/d");
    fs::create_dir_all(&params)?;
    let path = params.join(&case.key);
    fs::write(&path, &case.value)?;
    match case.failure.as_str() {
        "directory" => {
            fs::remove_file(&path)?;
            fs::create_dir(&path)?;
        }
        "unreadable" => mode(&path, 0)?,
        "unwritable" => mode(&params, 0o500)?,
        _ => {}
    }
    let mut argv = prefix.to_vec();
    argv.extend([binary.as_os_str().to_owned(), OsString::from(&case.key)]);
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(30));
    spec.set_os_env(BTreeMap::from([
        (
            OsString::from("PARAMS_ROOT"),
            params
                .parent()
                .ok_or("missing Params parent")?
                .as_os_str()
                .to_owned(),
        ),
        (OsString::from("OPENPILOT_PREFIX"), OsString::from("d")),
    ]));
    let output = runner.run(&spec)?;
    // Match the original ordering: repair fixture access, retain raw outputs and
    // invocation provenance, and only then reject status/encoding/format errors.
    mode(&params, 0o700)?;
    if path.is_file() {
        mode(&path, 0o600)?;
    }
    log(&directory.join("stdout.log"), &output.stdout)?;
    log(&directory.join("stderr.log"), &output.stderr)?;
    let quoted = argv
        .iter()
        .map(|arg| Ok(format!("    {}", crate::python_fs::json_string(arg)?)))
        .collect::<Result<Vec<_>>>()?;
    let invocation = format!(
        "{{\n  \"argv\": [\n{}\n  ],\n  \"exit\": {},\n  \"binary_sha256\": \"{:x}\"\n}}",
        quoted.join(",\n"),
        output.code,
        Sha256::digest(fs::read(binary)?)
    );
    fs::write(directory.join("invocation.json"), invocation)?;
    require(
        output.code == 0,
        format!(
            "identifier probe exited {}: {}",
            output.code,
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
    let stdout = String::from_utf8(output.stdout)?;
    let identifier =
        stdout.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}'));
    let bytes = identifier.as_bytes();
    require(
        bytes.len() == 20
            && &bytes[8..10] == b"--"
            && bytes[..8]
                .iter()
                .chain(&bytes[10..])
                .all(|byte| matches!(byte,b'0'..=b'9'|b'a'..=b'f')),
        format!("invalid identifier {identifier:?}"),
    )?;
    let result = json!({"prefix":&identifier[..8],"counter":if path.is_file(){hex(&fs::read(&path)?)}else{"<directory>".into()}});
    write_json(&directory.join("result.json"), &result)?;
    Ok(result)
}
struct Options {
    binary: PathBuf,
    original: PathBuf,
    output: PathBuf,
    runner: Vec<OsString>,
}
pub use crate::argparse_scan::ArgumentError;
fn parse_arguments(
    args: &[OsString],
) -> std::result::Result<crate::argparse_scan::ParsedOs, ArgumentError> {
    crate::argparse_scan::scan_os_with_positionals(
        args,
        &[crate::argparse_scan::OptionSpec::append("--runner")],
        3,
    )
}
impl Options {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut parsed = parse_arguments(args)?;
        if parsed.help {
            return Err(ArgumentError("help is handled by the CLI".into()).into());
        }
        Ok(Self {
            binary: PathBuf::from(&parsed.positionals[0]).canonicalize()?,
            original: PathBuf::from(&parsed.positionals[1]).canonicalize()?,
            output: resolve_output(Path::new(&parsed.positionals[2]))?,
            runner: parsed.append_values.remove("--runner").unwrap_or_default(),
        })
    }
}
pub use crate::path_resolve::resolve_non_strict as resolve_output;

pub fn run(args: &[String], runner: &mut dyn Runner) -> Result<Value> {
    run_os(&args.iter().map(OsString::from).collect::<Vec<_>>(), runner)
}
pub fn run_os(args: &[OsString], runner: &mut dyn Runner) -> Result<Value> {
    let options = Options::parse(args)?;
    let mut results = vec![];
    for (ordinal, case) in cases().iter().enumerate() {
        let index = ordinal / 4 % 10;
        let label = format!("{}-{index}-{}", case.key, case.failure);
        let root = options.output.join(&label);
        let expected = run_case_os(&options.original, case, &root.join("source"), &[], runner)?;
        let actual = run_case_os(
            &options.binary,
            case,
            &root.join("native"),
            &options.runner,
            runner,
        )?;
        require(
            actual == expected,
            format!("identifier mismatch {label}: source={expected} native={actual}"),
        )?;
        results.push(json!({"scenario":label,"result":"PASS"}));
        println!("{label} PASS");
    }
    let result = json!({"result":"PASS","cases":results});
    write_json(&options.output.join("result.json"), &result)?;
    Ok(result)
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if parse_arguments(&args)?.help {
        println!("Usage: check-logger-identifier BINARY ORIGINAL OUTPUT [--runner ARG ...]");
        return Ok(());
    }
    run_os(&args, &mut NativeRunner)?;
    Ok(())
}

#[cfg(test)]
mod cli_original {
    #[test]
    fn original_append_and_positional_grammar() {
        use std::ffi::OsString;
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/logger-identifier/cli.json"))
                .unwrap();
        for row in rows.as_array().unwrap() {
            let args = row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| OsString::from(v.as_str().unwrap()))
                .collect::<Vec<_>>();
            let result = super::parse_arguments(&args);
            assert_eq!(result.is_ok(), row["original_exit"] == 0, "{row}");
            if let Ok(parsed) = result {
                assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{row}");
                if !parsed.help {
                    assert_eq!(
                        parsed
                            .positionals
                            .iter()
                            .map(|v| v.to_str().unwrap())
                            .collect::<Vec<_>>(),
                        [
                            row["parsed"]["binary"].as_str().unwrap(),
                            row["parsed"]["original"].as_str().unwrap(),
                            row["parsed"]["output"].as_str().unwrap()
                        ],
                        "{row}"
                    );
                    assert_eq!(
                        parsed
                            .append_values
                            .get("--runner")
                            .cloned()
                            .unwrap_or_default(),
                        row["parsed"]["runner"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| OsString::from(v.as_str().unwrap()))
                            .collect::<Vec<_>>(),
                        "{row}"
                    );
                }
            }
        }
    }
}
