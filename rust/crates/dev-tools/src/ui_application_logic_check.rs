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
fn application_source_matches(bytes: &[u8]) -> bool {
    const ORIGINAL_SHA: &str = "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00";
    if bytes.len() == 38593 && sha(bytes) == ORIGINAL_SHA {
        return true;
    }
    if bytes.len() != 42048 {
        return false;
    }
    // #557의 정확한 font 변경만 역투영한다. FPS/stack 외 source도 기존 full SHA로 묶는다.
    let changes: [(&[u8], usize, &str, &[u8]); 3] = [
        (
            b"import datetime\n",
            55,
            "8a9a5a9e0ffcae3ade84cf1f1963d413f0386a668a6073def1127042ebe8012e",
            b"import datetime\n",
        ),
        (
            b"SOURCE_FONT_DIR = ASSETS_DIR.joinpath(\"fonts\")\n",
            3342,
            "05cae7f7d27b7197c55339220680abea2908aaa4999917925c35c30c3e195cd9",
            b"FONT_DIR = ASSETS_DIR.joinpath(\"fonts\")\n",
        ),
        (
            b"    if \"OPENPILOT_FONT_ROOT\" in os.environ:\n",
            114,
            "f00755b2aaaca701d75f1fada614f39b6e87ecfbbdcdaca28b08e377dae4aa1c",
            b"",
        ),
    ];
    let mut projected = Vec::with_capacity(38593);
    let mut previous = 0;
    for (anchor, length, expected, replacement) in changes {
        let mut positions = bytes
            .windows(anchor.len())
            .enumerate()
            .filter_map(|(index, value)| (value == anchor).then_some(index));
        let Some(start) = positions.next() else {
            return false;
        };
        if positions.next().is_some() || start < previous {
            return false;
        }
        let Some(changed) = bytes.get(start..start + length) else {
            return false;
        };
        if sha(changed) != expected {
            return false;
        }
        projected.extend_from_slice(&bytes[previous..start]);
        projected.extend_from_slice(replacement);
        previous = start + length;
    }
    projected.extend_from_slice(&bytes[previous..]);
    projected.len() == 38593 && sha(&projected) == ORIGINAL_SHA
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
    if !application_source_matches(&fs::read(
        root.join("openpilot/system/ui/lib/application.py"),
    )?) {
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
