//! Verify the real UI updater's queued exec error after its helper exits.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonError, JsonValue, JsonView};
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn lines(source: &str) -> impl Iterator<Item = &str> {
    source.split_terminator([
        '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
    ])
}
fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value.bytes().any(|byte| byte != b'-' && byte != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
fn object_field(row: &JsonValue, key: &str) -> Result<Option<JsonValue>> {
    let object = row.get(key);
    if object.as_ref().is_some_and(|value| !value.is_object()) {
        return Err(format!("Cargo {key} does not support object lookup").into());
    }
    Ok(object)
}
pub fn check(evidence: &Path, runner: &mut dyn Runner) -> Result<()> {
    let receipt = text(&evidence.join("unit-tests.jsonl"))?;
    let mut artifacts = vec![];
    for line in lines(&receipt) {
        let row = match JsonValue::parse(line) {
            Ok(row) => row,
            // The original catches JSONDecodeError, but not integer-limit errors.
            Err(JsonError::Syntax { .. }) => continue,
            Err(error) => return Err(error.into()),
        };
        if !row.is_object()
            || !row
                .get("reason")
                .is_some_and(|value| value.text_eq("compiler-artifact"))
        {
            continue;
        }
        if !object_field(&row, "target")?
            .and_then(|target| target.get("name"))
            .is_some_and(|name| name.text_eq("openpilot_ui_application"))
        {
            continue;
        }
        if !object_field(&row, "profile")?
            .and_then(|profile| profile.get("test"))
            .is_some_and(|value| truthy(&value))
        {
            continue;
        }
        if let Some(binary) = row.get("executable").filter(truthy) {
            artifacts.push(binary);
        }
    }
    if artifacts.len() != 1 {
        return Err("expected exactly one UI application test artifact".into());
    }
    let binary = artifacts[0]
        .to_utf8()
        .ok_or("UI application executable must be a Unicode path string")?;
    let trace = evidence.join("updater-handshake.trace");
    let mut command = CommandSpec::new(
        vec![
            "strace".into(),
            "--kill-on-exit".into(),
            "-f".into(),
            "-qq".into(),
            "-e".into(),
            "trace=accept4,waitid".into(),
            "-e".into(),
            "inject=accept4:delay_exit=500ms:when=1".into(),
            "-o".into(),
            trace.to_string_lossy().into_owned(),
            binary,
            "agnos_updater::worker::tests::missing_executable_is_visible_start_failure".into(),
            "--exact".into(),
            "--nocapture".into(),
        ],
        Duration::from_secs(20),
    );
    command.inherit_output = true;
    let result = runner.run(&command)?;
    if result.code != 0 {
        return Err(format!("UI updater strace check exited {}", result.code).into());
    }
    let trace_text = text(&trace)?;
    let trace_lines = lines(&trace_text).collect::<Vec<_>>();
    let delayed_pattern = Regex::new(r"^(\d+)\s+accept4.*EAGAIN.*\(DELAYED\)")?;
    let delayed = trace_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            delayed_pattern
                .captures(line)
                .map(|capture| (index, capture[1].to_owned()))
        })
        .collect::<Vec<_>>();
    if delayed.len() != 1 {
        return Err("expected one delayed EAGAIN in updater worker".into());
    }
    let (index, pid) = &delayed[0];
    let events_pattern = Regex::new(&format!(r"^{pid}\s+(accept4|waitid)\("))?;
    let events = trace_lines[index + 1..]
        .iter()
        .filter(|line| events_pattern.is_match(line))
        .collect::<Vec<_>>();
    if events.len() < 2 || !events[0].contains("waitid(") || !events[0].contains("CLD_EXITED") {
        return Err("updater worker must reap exited helper before accepting queued error".into());
    }
    if !events[1].contains("accept4(") || !Regex::new(r"= [0-9]+$")?.is_match(events[1]) {
        return Err("updater worker must accept queued error after helper exit".into());
    }
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] || args == ["-h"] {
        println!("Usage: check-ui-updater-handshake --evidence DIRECTORY");
        return Ok(());
    }
    let evidence = match args.as_slice() {
        [key, path] if key == "--evidence" => PathBuf::from(path),
        _ => return Err("expected --evidence DIRECTORY".into()),
    };
    check(&evidence, &mut NativeRunner)
}
