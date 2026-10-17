//! Source-repository-only upstream mirror. The guard precedes every gh call;
//! every update is checked twice for fast-forward safety and read back afterward.
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use std::{fs::OpenOptions, io::Write, path::Path, time::Duration};

const REPOSITORY: &str = "bin9208/openpilot";
const UPSTREAM: &str = "ajouatom/openpilot";
const BRANCH: &str = "carrot-wip";
#[derive(Debug, Serialize)]
pub struct Report {
    repository: String,
    branch: String,
    before: String,
    after: String,
    changed: bool,
    force: bool,
}
#[derive(Deserialize)]
struct Comparison {
    status: Option<String>,
    behind_by: Option<Box<RawValue>>,
    ahead_by: Option<Box<RawValue>>,
    base_commit: Option<Value>,
    merge_base_commit: Option<Value>,
}
fn valid_sha(value: &Value) -> Result<&str> {
    let text = value.as_str().ok_or("Invalid commit SHA")?;
    if text.len() != 40
        || !text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("Invalid commit SHA".into());
    }
    Ok(text)
}
fn positive_python_integer(value: &RawValue) -> bool {
    let token = value.get();
    // Python bool is an int, but JSON floating/exponent syntax produces float.
    // Keep the original integer token to avoid any u64 bound or f64 rounding.
    token == "true"
        || (token.bytes().all(|b| b.is_ascii_digit()) && token.bytes().any(|b| b != b'0'))
}
fn python_equal_zero(value: &RawValue) -> bool {
    let token = value.get();
    if token == "false" {
        return true;
    }
    // json.loads converts decimal/exponent numbers to IEEE f64; underflow and
    // negative zero must therefore compare just as they did in Python.
    token.parse::<f64>().is_ok_and(|number| number == 0.0)
}
/// `comparison_json` remains raw JSON so integer and floating spellings retain
/// the original Python contract without changing workspace Number semantics.
pub fn should_update(current: &Value, upstream: &Value, comparison_json: &str) -> Result<bool> {
    valid_sha(current)?;
    valid_sha(upstream)?;
    if current == upstream {
        return Ok(false);
    }
    let comparison: Comparison = serde_json::from_str(comparison_json)?;
    if comparison.status.as_deref() != Some("ahead")
        || !comparison
            .behind_by
            .as_deref()
            .is_some_and(python_equal_zero)
        || !comparison
            .ahead_by
            .as_deref()
            .is_some_and(positive_python_integer)
        || comparison.base_commit.as_ref().and_then(|v| v.get("sha")) != Some(current)
        || comparison
            .merge_base_commit
            .as_ref()
            .and_then(|v| v.get("sha"))
            != Some(current)
    {
        return Err(
            "Mirror is not a fast-forward of the recorded target; manual review required".into(),
        );
    }
    Ok(true)
}
fn api(runner: &mut dyn Runner, endpoint: &str, args: &[String]) -> Result<String> {
    let mut argv = vec!["gh".into(), "api".into(), endpoint.into()];
    argv.extend_from_slice(args);
    let output = runner.run(&CommandSpec::new(argv, Duration::from_secs(60)))?;
    if output.code != 0 {
        return Err(format!(
            "gh api {endpoint} exited {}: {}",
            output.code,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let json = String::from_utf8(output.stdout)?;
    // Validate JSON without resolving its numeric tokens into serde_json::Number.
    let _: Box<RawValue> = serde_json::from_str(&json)?;
    Ok(json)
}
fn reference(runner: &mut dyn Runner, repository: &str) -> Result<Value> {
    let response = api(
        runner,
        &format!("repos/{repository}/git/ref/heads/{BRANCH}"),
        &[],
    )?;
    let value: Value = serde_json::from_str(&response)?;
    value
        .pointer("/object/sha")
        .cloned()
        .ok_or_else(|| "missing reference object SHA".into())
}
pub fn sync(
    repository: Option<&str>,
    summary: Option<&Path>,
    runner: &mut dyn Runner,
) -> Result<Report> {
    if repository != Some(REPOSITORY) {
        return Err("This workflow is restricted to its explicitly configured fork".into());
    }
    let metadata: Value = serde_json::from_str(&api(runner, &format!("repos/{REPOSITORY}"), &[])?)?;
    if metadata
        .pointer("/parent/full_name")
        .and_then(Value::as_str)
        != Some(UPSTREAM)
    {
        return Err("Unexpected fork parent".into());
    }
    let current = reference(runner, REPOSITORY)?;
    let upstream = reference(runner, UPSTREAM)?;
    let comparison = if current == upstream {
        "{}".into()
    } else {
        let endpoint = format!(
            "repos/{REPOSITORY}/compare/{}...{}",
            current.as_str().ok_or("Invalid commit SHA")?,
            upstream.as_str().ok_or("Invalid commit SHA")?
        );
        api(runner, &endpoint, &[])?
    };
    let changed = should_update(&current, &upstream, &comparison)?;
    if changed {
        api(
            runner,
            &format!("repos/{REPOSITORY}/git/refs/heads/{BRANCH}"),
            &[
                "--method".into(),
                "PATCH".into(),
                "-f".into(),
                format!("sha={}", upstream.as_str().ok_or("Invalid commit SHA")?),
                "-F".into(),
                "force=false".into(),
            ],
        )?;
    }
    let actual = reference(runner, REPOSITORY)?;
    if actual != upstream {
        return Err("Post-sync commit does not match the observed upstream".into());
    }
    let report = Report {
        repository: REPOSITORY.into(),
        branch: BRANCH.into(),
        before: valid_sha(&current)?.into(),
        after: valid_sha(&actual)?.into(),
        changed,
        force: false,
    };
    println!("{}", serde_json::to_string(&report)?);
    // Preserve original failure order: verified stdout is emitted before a
    // potentially failing summary append, which may follow an actual update.
    if let Some(path) = summary {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        write!(
            file,
            "Fast-forward-only mirror result:\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&report)?
        )?;
    }
    Ok(report)
}
pub fn cli() -> Result<()> {
    let repository = std::env::var("GITHUB_REPOSITORY").ok();
    let summary = std::env::var_os("GITHUB_STEP_SUMMARY")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from);
    sync(repository.as_deref(), summary.as_deref(), &mut NativeRunner)?;
    Ok(())
}
