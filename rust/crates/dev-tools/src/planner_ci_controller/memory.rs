//! 기존 Python memory wrapper의 sanitizer 준비·해시·실행·한정 receipt를 소유한다.
use super::{
    plan::{self, Inputs, MEMORY_CHECK_NAMES},
    run_checks, write_json,
};
use crate::{
    path_resolve::resolve_non_strict,
    qa_process::Runner,
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

const ASAN_OPTIONS: &str = "detect_leaks=1:abort_on_error=1:strict_string_checks=1";
const UBSAN_OPTIONS: &str = "halt_on_error=1:print_stacktrace=1";
const SCOPE: &str = "generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented";

fn require_retired_sources_absent(root: &Path) -> Result<()> {
    for relative in [
        "rust/tools/check_plannerd_native_memory.py",
        "rust/tools/tests/test_plannerd_native_memory_controller.py",
    ] {
        match fs::symlink_metadata(root.join(relative)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(format!("retired Planner memory source reappeared: {relative}").into())
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

// Python shlex.quote와 같은 ASCII safe 문자 및 작은따옴표 escape다.
fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

/// leaf는 기존 memory core와 같은 Runner·owned process group·기한·growth guard를 쓴다.
/// sanitizer 환경은 trace 실행 wrapper에만 적용하며 Python 비교기에는 주입하지 않는다.
pub fn run(
    inputs: &Inputs,
    asan: &Path,
    output: &Path,
    growth: &BTreeMap<String, u64>,
    timeout: Duration,
    runner: &mut dyn Runner,
) -> Result<Value> {
    require_retired_sources_absent(&inputs.root)?;
    let mut sanitized = inputs.clone();
    sanitized.artifact = resolve_non_strict(&inputs.artifact)?;
    sanitized.source = resolve_non_strict(&inputs.source)?;
    sanitized.binding = resolve_non_strict(&inputs.binding)?;
    let output = resolve_non_strict(output)?;
    let provenance: Value =
        serde_json::from_slice(&fs::read(sanitized.artifact.join("provenance.json"))?)?;
    if !truth(
        provenance
            .get("sanitizers")
            .ok_or("missing sanitizers provenance")?,
    ) {
        return Err("an instrumented generated-C artifact is required".into());
    }
    let asan = asan.canonicalize()?;
    let wrappers = output.join("wrappers/examples");
    fs::create_dir_all(wrappers.parent().ok_or("missing wrappers parent")?)?;
    fs::create_dir(&wrappers)?;
    let mut identities = BTreeMap::new();
    for name in ["solver", "owner", "runtime"] {
        let binary = inputs
            .binaries
            .join("examples")
            .join(format!("plannerd_{name}_trace"))
            .canonicalize()?;
        let command = [
            "env".to_owned(),
            format!(
                "LD_PRELOAD={}",
                asan.to_str().ok_or("ASan path must be UTF-8")?
            ),
            format!("ASAN_OPTIONS={ASAN_OPTIONS}"),
            format!("UBSAN_OPTIONS={UBSAN_OPTIONS}"),
            binary.to_str().ok_or("trace path must be UTF-8")?.into(),
        ]
        .iter()
        .map(|value| quote(value))
        .collect::<Vec<_>>()
        .join(" ");
        let wrapper = wrappers.join(binary.file_name().ok_or("missing trace filename")?);
        fs::write(&wrapper, format!("#!/bin/sh\nexec {command} \"$@\"\n"))?;
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
        identities.insert(
            binary
                .to_str()
                .ok_or("trace path must be UTF-8")?
                .to_owned(),
            digest(&binary)?,
        );
    }
    sanitized.binaries = output.join("wrappers");
    let checks = plan::memory_checks(&sanitized, growth, timeout)?;
    let checked = run_checks(&checks, &output.join("comparison"), &sanitized.root, runner);
    // 실패 행을 보존한 뒤에도 같은 두 경로의 재등장을 거부한다.
    require_retired_sources_absent(&inputs.root)?;
    checked?;
    write_receipt(&sanitized.artifact, &asan, &output, &identities)
}

/// exit0만으로 성공시키지 않는다. 기존 wrapper처럼 세 행의 이름·순서·종료코드를 재확인한다.
pub fn write_receipt(
    artifact: &Path,
    asan: &Path,
    output: &Path,
    identities: &BTreeMap<String, String>,
) -> Result<Value> {
    let commands: Value =
        serde_json::from_slice(&fs::read(output.join("comparison/commands.json"))?)?;
    let rows = commands
        .as_array()
        .ok_or("memory command records must be an array")?;
    if rows.len() != MEMORY_CHECK_NAMES.len()
        || rows.iter().zip(MEMORY_CHECK_NAMES).any(|(row, name)| {
            row["name"].as_str() != Some(name) || row["returncode"].as_i64() != Some(0)
        })
    {
        return Err("memory requires successful abi/owner/fault command records in order".into());
    }
    let receipt = json!({
        "status":"PASS", "checks":MEMORY_CHECK_NAMES, "binary_sha256":identities,
        "artifact_sha256":digest(&artifact.join("manifest.json"))?,
        "asan_sha256":digest(asan)?, "scope":SCOPE
    });
    write_json(&output.join("result.json"), &receipt)?;
    Ok(receipt)
}
