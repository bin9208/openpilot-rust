//! Independently captured fixed-input GuiApplication FPS and stack parity gates.
//! Generic runtime methods remain pending; capture evidence is Linux x86_64.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
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
    Fps,
    Stack,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn load(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-application-logic/integrity.json"
    ))?;
    let row = &manifest[name];
    if Some(sha(bytes).as_str()) != row["gzip_sha256"].as_str() {
        return Err("compressed application corpus integrity mismatch".into());
    }
    let mut value = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(row["bytes"].as_u64().ok_or("missing corpus size")? + 1)
        .read_to_end(&mut value)?;
    if Some(sha(&value).as_str()) != row["sha256"].as_str() {
        return Err("original application corpus integrity mismatch".into());
    }
    Ok(value)
}
fn corpus(kind: Kind) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    macro_rules! fixture {
        ($kind:literal,$name:literal) => {
            load(
                include_bytes!(concat!(
                    "../tests/fixtures/ui-application-logic/",
                    $kind,
                    "-",
                    $name,
                    ".json.gz"
                )),
                concat!($kind, "-", $name, ".json.gz"),
            )?
        };
    }
    Ok(match kind {
        Kind::Fps => (
            fixture!("fps", "input"),
            fixture!("fps", "source"),
            fixture!("fps", "stdin"),
        ),
        Kind::Stack => (
            fixture!("stack", "input"),
            fixture!("stack", "source"),
            fixture!("stack", "stdin"),
        ),
    })
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn parse(text: &str) -> Result<JsonValue> {
    // Whole-original CLI captures admit depth9997; overwritten/ignored structures
    // still contribute to the decoder limit before equality or serialization.
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
        .split(|v| *v == b'/')
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
/// Compare the complete original fixed CLI corpus without running Python.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
) -> Result<String> {
    fs::create_dir_all(output)?;
    let root = resolve_non_strict(root)?;
    if sha(&fs::read(
        root.join("openpilot/system/ui/lib/application.py"),
    )?) != "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00"
    {
        return Err(
            "GuiApplication source changed; independently recapture original corpus".into(),
        );
    }
    let (input, source, stdin) = corpus(kind)?;
    let expected = parse(std::str::from_utf8(&source)?)?;
    if matches!(kind, Kind::Stack) {
        fs::write(output.join("input.json"), &input)?;
        fs::write(output.join("source.json"), &source)?;
    }
    let mut command = CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
    command.input = Some(stdin);
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    command.inherit_stderr = matches!(kind, Kind::Fps);
    let result = runner.run(&command)?;
    let stdout = text(result.stdout)?;
    // FPS check_output inherits raw stderr; it never decodes or records it.
    let stderr = if matches!(kind, Kind::Stack) {
        text(result.stderr)?
    } else {
        String::new()
    };
    if result.code != 0 {
        return Err(format!("application candidate exited {}", result.code).into());
    }
    let actual = match kind {
        Kind::Fps => {
            let actual = parse(&stdout)?;
            fs::write(output.join("input.json"), &input)?;
            fs::write(output.join("source.json"), &source)?;
            // Python evaluates dumps before calling write_text: serialization failure
            // must preserve an existing native.json, not truncate or create it.
            let encoded = json::pretty_with_limit(&actual, 0, 996)?;
            fs::write(output.join("native.json"), encoded)?;
            actual
        }
        Kind::Stack => {
            fs::write(output.join("native.json"), &stdout)?;
            fs::write(
                output.join("native.stderr"),
                if stderr.is_empty() {
                    "(no stderr)\n"
                } else {
                    &stderr
                },
            )?;
            parse(&stdout)?
        }
    };
    if !json::equal(&expected, &actual) {
        return Err("original/native application logic differs".into());
    }
    if matches!(kind, Kind::Stack) {
        fs::write(
            output.join("result.json"),
            include_str!("../tests/fixtures/ui-application-logic/stack-result.json"),
        )?;
    }
    let printed = match kind {
        Kind::Fps => "PASS: 112 unchanged-source/native FPS warning throttle and strict-mode threshold decisions\n",
        Kind::Stack => "PASS: 19 actual-source/native stack lifecycle, duplicate/index rejection and callback operations\n",
    };
    Ok(printed.into())
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
            "usage: {} --binary BINARY --output OUTPUT",
            if matches!(kind, Kind::Fps) {
                "check-ui-fps"
            } else {
                "check-ui-stack"
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
            Path::new(&args.values["--binary"]),
            Path::new(&args.values["--output"]),
            kind,
            &mut NativeRunner
        )?
    );
    Ok(())
}
