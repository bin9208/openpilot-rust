//! Native source/candidate identifier parsing and Params I/O boundary checker.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
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
    argv.extend([binary.display().to_string(), case.key.clone()]);
    let mut spec = CommandSpec::new(argv.clone(), Duration::from_secs(30));
    spec.env = BTreeMap::from([
        (
            "PARAMS_ROOT".into(),
            params
                .parent()
                .ok_or("missing Params parent")?
                .display()
                .to_string(),
        ),
        ("OPENPILOT_PREFIX".into(), "d".into()),
    ]);
    let output = runner.run(&spec)?;
    // Match the original ordering: repair fixture access, retain raw outputs and
    // invocation provenance, and only then reject status/encoding/format errors.
    mode(&params, 0o700)?;
    if path.is_file() {
        mode(&path, 0o600)?;
    }
    log(&directory.join("stdout.log"), &output.stdout)?;
    log(&directory.join("stderr.log"), &output.stderr)?;
    write_json(
        &directory.join("invocation.json"),
        &json!({"argv":argv,"exit":output.code,"binary_sha256":format!("{:x}",Sha256::digest(fs::read(binary)?))}),
    )?;
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
    runner: Vec<String>,
}
impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut positional = vec![];
        let mut runner = vec![];
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            i += 1;
            if arg == "--runner" {
                runner.push(args.get(i).ok_or("--runner requires a value")?.clone());
                i += 1;
            } else if let Some(value) = arg.strip_prefix("--runner=") {
                runner.push(value.into());
            } else {
                require(
                    !arg.starts_with('-'),
                    format!("unknown identifier option {arg}"),
                )?;
                positional.push(arg);
            }
        }
        require(positional.len() == 3, "expected BINARY ORIGINAL OUTPUT")?;
        Ok(Self {
            binary: PathBuf::from(positional[0]).canonicalize()?,
            original: PathBuf::from(positional[1]).canonicalize()?,
            output: resolve_output(Path::new(positional[2]))?,
            runner,
        })
    }
}
pub use crate::path_resolve::resolve_non_strict as resolve_output;

pub fn run(args: &[String], runner: &mut dyn Runner) -> Result<Value> {
    let options = Options::parse(args)?;
    let mut results = vec![];
    for (ordinal, case) in cases().iter().enumerate() {
        let index = ordinal / 4 % 10;
        let label = format!("{}-{index}-{}", case.key, case.failure);
        let root = options.output.join(&label);
        let expected = run_case(&options.original, case, &root.join("source"), &[], runner)?;
        let actual = run_case(
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(),[arg]if arg=="--help"||arg=="-h") {
        println!("Usage: check-logger-identifier BINARY ORIGINAL OUTPUT [--runner ARG ...]");
        return Ok(());
    }
    run(&args, &mut NativeRunner)?;
    Ok(())
}
