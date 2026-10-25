//! Independent dynamic-reference/private-ABI EGL gate; generic UI runtime remains separate.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const CASES: [&str; 7] = [
    "success",
    "retry",
    "fallback",
    "fail",
    "retry-fail",
    "no-display",
    "init-fail",
];
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn parse(text: &str) -> Result<JsonValue> {
    // Whole unchanged Python3.12.15 CLI admits total depth9997; even ignored
    // fields must be decoded and cannot bypass the original decoder limit.
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
                depth = depth.saturating_sub(1);
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
        .split(|b| *b == b'/')
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/');
        }
        out.extend(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    OsString::from_vec(out).into()
}

fn lines(text: &str) -> Vec<&str> {
    // Python str.splitlines also recognizes Unicode line/paragraph separators,
    // vertical tab, form feed and information separators. CR was normalized.
    let mut lines = vec![];
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if matches!(
            c,
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            lines.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

fn traces(text: &str) -> Result<JsonValue> {
    let rows = lines(text)
        .into_iter()
        .map(parse)
        .collect::<Result<Vec<_>>>()?;
    Ok(JsonValue::parse(&format!(
        "[{}]",
        rows.iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))?)
}
fn pins(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let pins: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/ui-egl/policy-pins.json"))?;
    for (path, hash) in pins["sources"]
        .as_object()
        .ok_or("missing EGL reference source pins")?
    {
        let observed = format!("{:x}", Sha256::digest(fs::read(root.join(path))?));
        if hash.as_str() != Some(&observed) {
            return Err(format!(
                "{path} changed; independently revalidate the Rust EGL reference policy"
            )
            .into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing EGL source dependencies")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| matches!(name, "cffi" | "pycparser"))
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(records)? != pins["lock"] {
        return Err(
            "EGL source dependencies changed; independently revalidate the Rust reference policy"
                .into(),
        );
    }
    Ok(())
}
/// Run fresh reference/candidate processes against the same freshly built actual libraries.
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let root = resolve_non_strict(root)?;
    pins(&root)?;
    let mut compile = CommandSpec::new_os(
        vec![
            OsString::from("g++"),
            "-shared".into(),
            "-fPIC".into(),
            "-std=c++17".into(),
            root.join("rust/tools/ui_qa/egl_fixture.cc")
                .into_os_string(),
            "-ldl".into(),
            "-o".into(),
            output.join("libEGL.so").into_os_string(),
        ],
        Duration::MAX,
    );
    compile.inherit_stdin = true;
    compile.stdout_limit = usize::MAX;
    compile.stderr_limit = usize::MAX;
    let built = runner.run_to_file(&compile, &output.join("build.log"))?;
    if built.code != 0 {
        return Err(format!("EGL fixture compiler exited {}", built.code).into());
    }
    let gles = output.join("libGLESv2.so");
    if !gles.try_exists()? {
        std::os::unix::fs::symlink("libEGL.so", &gles)?;
    }
    let mut results = vec![];
    let mut accepted = true;
    for case in CASES {
        let mut states = vec![];
        let mut trace_values = vec![];
        for (lane, executable) in [("source", reference), ("native", binary)] {
            let trace = output.join(format!("{case}-{lane}.jsonl"));
            fs::write(&trace, b"")?;
            let mut env = BTreeMap::new();
            env.insert(OsString::from("EGL_CASE"), OsString::from(case));
            env.insert(
                OsString::from("EGL_TRACE"),
                resolve_non_strict(&trace)?.into_os_string(),
            );
            env.insert(
                OsString::from("LD_LIBRARY_PATH"),
                resolve_non_strict(&output)?.into_os_string(),
            );
            env.insert(OsString::from("PYTHONPATH"), root.as_os_str().to_owned());
            let mut command =
                CommandSpec::new_os(vec![lexical(executable).into_os_string()], Duration::MAX);
            command.inherit_stdin = true;
            command.stdout_limit = usize::MAX;
            command.stderr_limit = usize::MAX;
            command.set_os_env(env);
            let result = runner.run(&command)?;
            let stdout = text(result.stdout)?;
            let stderr = text(result.stderr)?;
            if result.code != 0 {
                return Err(format!("EGL {case}/{lane} exited {}", result.code).into());
            }
            fs::write(output.join(format!("{case}-{lane}.stdout")), &stdout)?;
            fs::write(
                output.join(format!("{case}-{lane}.stderr")),
                if stderr.is_empty() {
                    "(no stderr)\n"
                } else {
                    &stderr
                },
            )?;
            states.push(parse(&stdout)?);
            trace_values.push(traces(&text(fs::read(&trace)?)?)?);
        }
        let state_equal = json::equal(&states[0], &states[1]);
        let trace_equal = json::equal(&trace_values[0], &trace_values[1]);
        let fd = states[1]
            .get("fd_delta")
            .ok_or("missing candidate fd_delta")?;
        let row=format!("{{\"case\": \"{case}\", \"state_equal\": {state_equal}, \"trace_equal\": {trace_equal}, \"fd_delta\": {}}}",fd.to_json()?);
        printed.write_all(row.as_bytes())?;
        printed.write_all(b"\n")?;
        printed.flush()?;
        accepted &= state_equal && trace_equal && json::equal(&fd, &JsonValue::parse("0")?);
        results.push(row);
    }
    let results = JsonValue::parse(&format!("[{}]", results.join(", ")))?;
    let pretty = json::pretty_with_limit(&results, 0, 996)?;
    fs::write(output.join("results.json"), pretty)?;
    if !accepted {
        return Err("source/native EGL state, trace or descriptor cleanup differs".into());
    }
    printed.write_all(b"PASS: 7 actual-source/native EGL lifecycle cases, NV12 attributes, retry, extension fallback, errors and duplicated-FD cleanup\n")?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-ui-egl --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let reference = std::env::current_exe()?
        .parent()
        .ok_or("EGL checker executable has no parent")?
        .join("ui-egl-reference");
    check(
        &root,
        Path::new(&args.values["--binary"]),
        &reference,
        Path::new(&args.values["--output"]),
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
