//! Shared UI orchestration. One original subchecker remains an explicit dependency;
//! this module replaces only their caller, never their validation behavior.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    python_fs,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const SOURCE: &str = "rust/tools/check_ui_framework.py";
const SOURCE_SHA: &str = "657c638fc5cc2a071f45b8482ecb1866e3b6ee26eef4a04746a52226e5bdfcd3";
const REQUIRED: u64 = 26 * 1024 * 1024 * 1024;
const CHECKS: [(&str, Option<&str>, &str, bool); 13] = [
    ("widgets", None, "widget_trace", false),
    ("scroll", None, "scroll_trace", false),
    ("navigation", None, "navigation_trace", false),
    ("stack", None, "stack_trace", false),
    ("scroller", None, "scroller_trace", false),
    ("render", None, "ui_render", true),
    ("forms", None, "form_render", true),
    ("network", None, "network_render", true),
    ("polygon", None, "polygon_render", true),
    ("diagnostics", None, "diagnostic_render", true),
    (
        "application",
        Some("check_ui_application.py"),
        "application_run",
        true,
    ),
    ("fps", None, "fps_trace", false),
    ("egl", None, "egl_trace", false),
];
fn validate_source(root: &Path) -> Result<()> {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui-framework/original.json"))?;
    if corpus["source_sha256"] != SOURCE_SHA {
        return Err("UI framework original provenance changed".into());
    }
    match fs::read(root.join(SOURCE)) {
        Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != SOURCE_SHA => {
            Err("UI framework original source changed; recapture its process contract".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
// PathBuf retains './' in a relative prefix while Python Path drops it. Keep
// '..' and exactly two leading slashes; neither caller resolves target/output.
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut result = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !result.is_empty() && !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    if result.is_empty() {
        result.push(b'.');
    }
    OsString::from_vec(result).into()
}
fn strings(values: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
/// Return the first child's original return code. A negative code represents a
/// signal; the CLI retains Python SystemExit's Unix modulo-256 exit behavior.
/// `free_bytes` is injected for exact boundary and failure-artifact comparisons.
#[allow(clippy::too_many_arguments)]
pub fn check(
    root: &Path,
    target: &Path,
    output: &Path,
    display: &OsStr,
    python: &OsStr,
    runner: &mut dyn Runner,
    free_bytes: &mut dyn FnMut(&Path) -> Result<u64>,
    printed: &mut dyn Write,
) -> Result<i32> {
    let target = lexical(target);
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    validate_source(root)?;
    let mut ledger = Vec::new();
    for (name, script, binary, needs_display) in CHECKS {
        if name == "egl" {
            let available = free_bytes(&output)?;
            if available < REQUIRED {
                return Err("requires 25GiB floor plus 1GiB fixture build allowance".into());
            }
            fs::write(
                output.join("egl-build-disk.json"),
                format!(
                    "{{\n  \"available_bytes\": {available},\n  \"required_bytes\": {REQUIRED}\n}}"
                ),
            )?;
        }
        let mut argv = if let Some(script) = script {
            vec![
                python.to_os_string(),
                root.join("rust/tools").join(script).into_os_string(),
            ]
        } else {
            vec![lexical(&target.join(format!("check-ui-{name}"))).into_os_string()]
        };
        argv.extend([
            OsString::from("--binary"),
            lexical(&target.join("examples").join(binary)).into_os_string(),
            OsString::from("--output"),
            lexical(&output.join(name)).into_os_string(),
        ]);
        if needs_display {
            argv.extend([OsString::from("--display"), display.to_os_string()]);
        }
        let invocation = strings(&argv)?;
        let mut command = CommandSpec::new_os(argv, Duration::MAX);
        command.cwd = Some(root.to_owned());
        command.inherit_stdin = true;
        command.stdout_limit = usize::MAX;
        command.stderr_limit = usize::MAX;
        let log = lexical(&output.join(format!("{name}.log")));
        let result = runner.run_to_file_with_prefix(
            &command,
            &log,
            format!("Invocation: {invocation}\n").as_bytes(),
        )?;
        let row = format!("{{\"scenario\": \"{name}\", \"invocation\": {invocation}, \"exit_code\": {}, \"artifact\": {}}}", result.code, python_fs::json_string(log.as_os_str())?);
        ledger.push(row);
        fs::write(
            output.join("invocations.json"),
            json::pretty_with_limit(
                &JsonValue::parse(&format!("[{}]", ledger.join(", ")))?,
                0,
                996,
            )?,
        )?;
        writeln!(printed, "{name}: exit {}", result.code)?;
        printed.flush()?;
        if result.code != 0 {
            return Ok(result.code);
        }
    }
    writeln!(printed,"PASS: complete shared UI source/native state, render, input, native lifecycle, recording, logging and EGL gates")?;
    Ok(0)
}
pub fn cli() -> Result<i32> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--target", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--display", true),
            OptionSpec::value("--python", false),
        ],
    )?;
    if args.help {
        println!("usage: check-ui-framework --target TARGET --output OUTPUT --display DISPLAY [--python PYTHON]");
        return Ok(0);
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = crate::path_resolve::resolve_non_strict(&root)?;
    // A native executable has no sys.executable. Keep this dependency explicit:
    // callers can supply the same interpreter used for the remaining tools.
    let python = args
        .values
        .get("--python")
        .cloned()
        .or_else(|| std::env::var_os("OPENPILOT_REFERENCE_PYTHON"))
        .unwrap_or_else(|| "python3".into());
    check(
        &root,
        Path::new(&args.values["--target"]),
        Path::new(&args.values["--output"]),
        &args.values["--display"],
        &python,
        &mut NativeRunner,
        &mut |path| {
            let stat = rustix::fs::statvfs(path)?;
            stat.f_bavail
                .checked_mul(stat.f_frsize)
                .ok_or_else(|| "UI output free-space count overflow".into())
        },
        &mut std::io::stdout().lock(),
    )
}
