//! Native CI routing checks. YAML scalars deliberately remain strings, matching
//! the former PyYAML BaseLoader rather than YAML's implicit type resolution.
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::ExitStatus,
    time::Duration,
};
use yaml_rust2::parser::{Event, Parser};

pub fn parse(source: &str) -> Result<Value, String> {
    let mut parser = Parser::new_from_str(source);
    let mut events = Vec::new();
    loop {
        let (event, _) = parser.next_token().map_err(|e| e.to_string())?;
        let done = event == Event::StreamEnd;
        events.push(event);
        if done {
            break;
        }
    }
    fn node(
        events: &[Event],
        position: &mut usize,
        anchors: &mut BTreeMap<usize, Value>,
    ) -> Result<Value, String> {
        let event = events.get(*position).ok_or("missing YAML value")?;
        *position += 1;
        let (value, anchor) = match event {
            Event::Scalar(text, _, anchor, _) => (Value::String(text.clone()), *anchor),
            Event::Alias(anchor) => {
                return anchors
                    .get(anchor)
                    .cloned()
                    .ok_or_else(|| format!("unresolved YAML alias {anchor}"));
            }
            Event::SequenceStart(anchor, _) => {
                let mut values = Vec::new();
                while !matches!(events.get(*position), Some(Event::SequenceEnd)) {
                    values.push(node(events, position, anchors)?);
                }
                *position += 1;
                (Value::Array(values), *anchor)
            }
            Event::MappingStart(anchor, _) => {
                let mut values = Map::new();
                while !matches!(events.get(*position), Some(Event::MappingEnd)) {
                    let key = node(events, position, anchors)?;
                    let key = key
                        .as_str()
                        .ok_or("workflow mapping key is not a scalar")?
                        .to_owned();
                    values.insert(key, node(events, position, anchors)?);
                }
                *position += 1;
                (Value::Object(values), *anchor)
            }
            _ => return Err(format!("unexpected YAML value event {event:?}")),
        };
        if anchor != 0 {
            anchors.insert(anchor, value.clone());
        }
        Ok(value)
    }
    let mut position = 0;
    if events.get(position) != Some(&Event::StreamStart) {
        return Err("missing YAML stream".into());
    }
    position += 1;
    if events.get(position) == Some(&Event::StreamEnd) {
        return Ok(Value::Null);
    }
    if events.get(position) != Some(&Event::DocumentStart) {
        return Err("missing YAML document".into());
    }
    position += 1;
    let result = node(&events, &mut position, &mut BTreeMap::new())?;
    if events.get(position) != Some(&Event::DocumentEnd)
        || events.get(position + 1) != Some(&Event::StreamEnd)
    {
        return Err("expected exactly one YAML document".into());
    }
    Ok(result)
}
fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn absent(value: &Value, keys: &[&str]) -> Result<(), String> {
    for key in keys {
        require(
            value.get(*key).is_none(),
            format!("unexpected policy key {key}"),
        )?;
    }
    Ok(())
}
fn strings(value: &Value) -> Result<BTreeSet<&str>, String> {
    value
        .as_array()
        .ok_or_else(|| "expected policy list".to_owned())?
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| "expected string policy item".to_owned())
        })
        .collect()
}
fn expected_strings<'a>(values: &[&'a str]) -> BTreeSet<&'a str> {
    values.iter().copied().collect()
}

/// #296 routing consumes the pinned #502 interfaces; it adds no ARM transport.
pub fn uses_native_action(job: &Value, step: &Value, name: &str) -> bool {
    let prefix = match job["env"].get("CI296_ALIAS_BASENAME") {
        Some(alias) => {
            let Some(alias) = alias
                .as_str()
                .filter(|alias| alias.starts_with(".ci296-source-") && !alias.contains('/'))
            else {
                return false;
            };
            format!("./{alias}/source")
        }
        None => ".".into(),
    };
    step["uses"] == format!("{prefix}/.github/actions/{name}")
}

pub const LOCAL_RUNNER: [&str; 4] = ["self-hosted", "Linux", "X64", "dev-md-local"];
pub const HOSTED_RUNNER: &str = "ubuntu-24.04";
pub const HOSTED_ARM_RUNNER: &str = "ubuntu-24.04-arm";
pub const DARWIN_RUNNER: &str = "macos-15";
fn approved_runner(runner: &Value) -> bool {
    runner == &serde_json::json!(LOCAL_RUNNER) || runner.as_str() == Some(HOSTED_RUNNER)
}
fn source_boundary_name(name: &str) -> bool {
    matches!(
        name,
        "Deny fork source and require registered local runner"
            | "Deny fork source and require GitHub-hosted Linux X64"
            | "Deny fork source and require GitHub-hosted Ubuntu X64 runner"
            | "Deny fork source and require GitHub-hosted Linux ARM64"
    )
}
pub const TRUSTED_SOURCE: &str = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}";
pub const TRUSTED_SUCCESS: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (success()) }}";
pub const TRUSTED_ALWAYS: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always()) }}";
pub const FORK_DENIAL: &str = "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied on persistent bin-actions' >&2; exit 1; }\n";
const ARM_FORK_DENIAL: &str = "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied on hosted ARM' >&2; exit 1; }\n";
// reusable의 github context는 caller이다. 단순 trusted=true 입력으로 승인하지 않는다.
pub const NATIVE_ARM_MAIN_CONTEXT: &str = r#"test "${CI296_NATIVE_ARM_CALLER:-}" = rust.yml
test "${GITHUB_REPOSITORY:-}" = bin9208/openpilot-rust
test "${GITHUB_WORKFLOW_REF:-}" = "bin9208/openpilot-rust/.github/workflows/rust.yml@${GITHUB_REF:-}"
case "${GITHUB_EVENT_NAME:-}" in
  push) [[ ${GITHUB_REF:-} == refs/heads/dev || ${GITHUB_REF:-} == refs/heads/main ]] ;;
  pull_request)
    [[ ${GITHUB_REF:-} =~ ^refs/pull/[1-9][0-9]*/merge$ &&
       ( ${GITHUB_BASE_REF:-} == dev || ${GITHUB_BASE_REF:-} == main ) &&
       ${CI296_PR_HEAD_REPOSITORY:-} == bin9208/openpilot-rust ]] ;;
  workflow_dispatch)
    case "${GITHUB_REF:-}" in refs/heads/?*|refs/tags/?*) ;; *) exit 1 ;; esac ;;
  *) exit 1 ;;
esac
"#;
fn validate_hosted_arm_boundary(job: &Value) -> Result<(), String> {
    require(
        job["env"]["CI296_NATIVE_ARM"] == "1" && job["runs-on"] == HOSTED_ARM_RUNNER,
        "native ARM requires explicit intent and its exact hosted label",
    )?;
    let first = job["steps"]
        .as_array()
        .and_then(|steps| steps.first())
        .ok_or("missing native ARM boundary")?;
    source_result_binding(first)?;
    let context = match job["env"].get("CI296_NATIVE_ARM_CALLER") {
        None => {
            "test \"$GITHUB_EVENT_NAME\" = push\ntest \"$GITHUB_REF\" = refs/heads/codex/feat-523-python-rust\n"
        }
        Some(value) if value == "rust.yml" => {
            require(
                job["env"]["CI296_PR_HEAD_REPOSITORY"]
                    == "${{ github.event.pull_request.head.repo.full_name }}",
                "native ARM PR head repository must bind the caller event",
            )?;
            NATIVE_ARM_MAIN_CONTEXT
        }
        _ => return Err("unknown native ARM caller scope".into()),
    };
    let expected = format!(
        "{ARM_FORK_DENIAL}set -euo pipefail\ntest \"$GITHUB_ACTIONS\" = true\ntest \"$GITHUB_REPOSITORY\" = bin9208/openpilot-rust\ntest \"$RUNNER_ENVIRONMENT\" = github-hosted\ntest \"$RUNNER_OS\" = Linux\ntest \"$RUNNER_ARCH\" = ARM64\ntest \"$(uname -s)\" = Linux\ntest \"$(uname -m)\" = aarch64\n{context}"
    );
    require(
        first["name"] == "Deny fork source and require GitHub-hosted Linux ARM64"
            && first["run"]
                .as_str()
                .is_some_and(|body| body.starts_with(&expected)),
        "native ARM guards must execute immediately after fork refusal",
    )
}
fn source_result_binding(step: &Value) -> Result<(), String> {
    absent(step, &["if", "continue-on-error"])?;
    require(
        step["env"]["SOURCE_ALLOWED"] == TRUSTED_SOURCE,
        "missing exact source trust binding",
    )?;
    require(
        step["run"]
            .as_str()
            .is_some_and(|s| {
                // 승인된 오류 문구만 허용한다. shell prelude는 이 한 줄 외에 허용하지 않는다.
                let s = s.strip_prefix("set -euo pipefail\n").unwrap_or(s);
                [
                    FORK_DENIAL,
                    ARM_FORK_DENIAL,
                    "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for UI recording qualification' >&2; exit 1; }\n",
                    "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for trusted docs validation' >&2; exit 1; }\n",
                    "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for setup qualification' >&2; exit 1; }\n",
                    "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for normal40 qualification' >&2; exit 1; }\n",
                    "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied for logging retained qualification' >&2; exit 1; }\n",
                ]
                .iter()
                .any(|prefix| s.starts_with(prefix))
            }),
        "fork source must fail before other commands",
    )
}
// A prefix alone would also admit `trusted && (false) || always()`. Require
// the entire remaining expression to stay inside the guarded parentheses.
fn trusted_condition(condition: &str) -> bool {
    if condition == TRUSTED_SOURCE {
        return true;
    }
    let Some(body)=condition.strip_prefix("${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (").and_then(|s|s.strip_suffix(" }}")) else {return false;};
    let mut depth = 1usize;
    let mut quote = None;
    let mut chars = body.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if let Some(active) = quote {
            if ch == active {
                if chars.peek().is_some_and(|(_, next)| *next == active) {
                    chars.next();
                } else {
                    quote = None;
                }
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                if depth == 0 && index + ch.len_utf8() != body.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && quote.is_none()
}
fn private_source_condition(body: &str) -> String {
    format!(
        "${{{{ ({}) && ({body}) }}}}",
        TRUSTED_SOURCE
            .trim_start_matches("${{ ")
            .trim_end_matches(" }}")
    )
}
fn private_upload_conditions(steps: &[Value]) -> Result<Vec<String>, String> {
    let mut conditions = vec!["always()".into()];
    // Match the maintained direct-upload order, then nested provider receipts.
    for nested in [false, true] {
        for step in steps {
            let called = step["uses"].as_str().unwrap_or("");
            let selected = if nested {
                called.ends_with("/.github/actions/native-eigen")
            } else {
                called.starts_with("actions/upload-artifact@")
            };
            if selected {
                let id = step["id"]
                    .as_str()
                    .ok_or("upload receipt has no explicit step ID")?;
                conditions.extend([
                    format!("steps.{id}.outcome == 'success'"),
                    format!("steps.{id}.outputs.artifact-id != ''"),
                    format!("steps.{id}.outputs.artifact-digest != ''"),
                ]);
            }
        }
    }
    Ok(conditions)
}

// main8과 #572 final-two의 정확 두 alias는 같은 strict SDK validator를 사용한다.
fn arm_sdk530_role(job: &Value) -> bool {
    if job["env"]["CI296_NATIVE_ARM"] != "1" {
        return false;
    }
    let alias = job["env"]["CI296_ALIAS_BASENAME"].as_str();
    match job["env"].get("CI296_NATIVE_ARM_CALLER") {
        Some(caller) if caller == "rust.yml" => matches!(
            alias,
            Some(
                ".ci296-source-rust--planner-runtime"
                    | ".ci296-source-rust--carrot-man-runtime"
                    | ".ci296-source-rust--webrtc-runtime"
                    | ".ci296-source-rust--webcam-runtime"
                    | ".ci296-source-rust--xiaoge-runtime"
                    | ".ci296-source-rust--radard-runtime"
                    | ".ci296-source-rust--radar-arm"
                    | ".ci296-source-rust--carrot-navi-arm"
            )
        ),
        None => matches!(
            alias,
            Some(".ci296-source-rust--radar-arm" | ".ci296-source-rust--carrot-navi-arm")
        ),
        _ => false,
    }
}

fn validate_carrot_x64_sdk530_steps(steps: &[Value]) -> Result<(), String> {
    let stage = |id: &str| {
        let found = steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s["id"] == id)
            .collect::<Vec<_>>();
        require(
            found.len() == 1,
            format!("CarrotMan SDK stage missing or duplicated: {id}"),
        )?;
        Ok::<_, String>(found[0])
    };
    let (before, first) = stage("carrot530_sdk")?;
    let (after, last) = stage("carrot530_sdk_final")?;
    for (step, name, condition, digest) in [
        (
            first,
            "Prepare exact fresh x64 SDK before CarrotMan cold bootstrap",
            TRUSTED_SOURCE.to_owned(),
            "3ac35298662ca18b12658fead672e2b4deacbb2f72e1ec68699794e167f2502f",
        ),
        (
            last,
            "Verify whole x64 SDK after every attempted CarrotMan check",
            private_source_condition(
                "always() && steps.carrot530_sdk.outcome != 'skipped' && env.CI296_SOURCE != ''",
            ),
            "4d7305f25cbb9f9de394305c1aa4514c222aa2d6f3d02b623d0ad201bac161c7",
        ),
    ] {
        absent(step, &["continue-on-error", "uses"])?;
        let body = step["run"]
            .as_str()
            .ok_or("missing CarrotMan SDK command")?;
        require(
            step["name"] == name
                && step["if"] == condition
                && step["shell"] == "bash"
                && step["working-directory"] == "${{ env.CI296_SOURCE }}"
                && step["env"] == serde_json::json!({"CI296_HOST_CONTROL":"1"})
                && format!("{:x}", Sha256::digest(body.as_bytes())) == digest,
            "CarrotMan SDK source/owner/freshness/admission/proxy/seal boundary changed",
        )?;
    }
    let dependency = steps
        .iter()
        .position(|s| s["name"] == "Reserve runner capacity and install pinned dependencies")
        .ok_or("missing CarrotMan dependency stage")?;
    let seal = steps
        .iter()
        .position(|s| s["id"] == "ci296_source_seal")
        .ok_or("missing CarrotMan source seal")?;
    require(
        before < dependency && dependency < after && after < seal,
        "CarrotMan SDK must surround maintained work before the source seal",
    )?;
    let body = steps[dependency]["run"]
        .as_str()
        .ok_or("missing CarrotMan dependencies")?;
    require(body.ends_with("test \"$CI_PREPARED_RUST_ROOT\" = \"$OPENPILOT_CI_PREPARED_RUST_ROOT\"\ntest \"$CI_PREPARED_RUST_MANIFEST\" = \"$OPENPILOT_CI_PREPARED_RUST_MANIFEST\"\nsha256sum --check --strict --quiet -- \"$CI_PREPARED_RUST_MANIFEST\"\n")
        && !body.contains("rustup toolchain install"),
        "CarrotMan dependencies must verify the prepared SDK without reinstalling it")
}

fn validate_arm_sdk530_steps(job: &Value, steps: &[Value]) -> Result<(), String> {
    let index = |label: &str| {
        let positions = steps
            .iter()
            .enumerate()
            .filter_map(|(i, step)| (step["name"] == label).then_some(i))
            .collect::<Vec<_>>();
        require(
            positions.len() == 1,
            format!("ARM SDK530 stage missing or duplicated: {label}"),
        )?;
        Ok::<usize, String>(positions[0])
    };
    let before = index("Prepare exact fresh ARM 1.94 SDK for all eight admitted roles")?;
    let after = index("Verify ARM SDK after every attempted maintained command")?;
    let dependency = index(match job["env"]["CI296_ALIAS_BASENAME"].as_str() {
        Some(".ci296-source-rust--planner-runtime") => {
            "Reserve capacity and install original planner dependencies"
        }
        Some(".ci296-source-rust--carrot-man-runtime") => {
            "Reserve runner capacity and install pinned dependencies"
        }
        Some(".ci296-source-rust--webrtc-runtime") => {
            "Reserve capacity and install pinned source and native providers"
        }
        Some(".ci296-source-rust--webcam-runtime") => {
            "Install exact source closure and native capture providers"
        }
        Some(".ci296-source-rust--xiaoge-runtime") => {
            "Reserve capacity and install Xiaoge build prerequisites"
        }
        Some(".ci296-source-rust--radard-runtime") => {
            "Reserve capacity and install original oracle dependencies"
        }
        Some(".ci296-source-rust--radar-arm") => {
            "Reserve runner capacity and install generic ARM dependencies"
        }
        Some(".ci296-source-rust--carrot-navi-arm") => {
            "Reserve runner capacity and install native ARM build dependencies"
        }
        _ => return Err("unsupported ARM SDK530 role".into()),
    })?;
    let finish = index("Finish native ARM execution evidence")?;
    let seal = index("Seal private source and index after maintained checks")?;
    let first_maintained = steps
        .iter()
        .position(|step| step["shell"] == "openpilot-local-shell {0}")
        .ok_or("missing ARM SDK530 maintained command")?;
    require(
        before < first_maintained
            && before < dependency
            && dependency < after
            && after < finish
            && finish < seal,
        "ARM SDK530 preparation and finalizer order changed",
    )?;
    for (at, id, minutes, phase, condition) in [
        (
            before,
            "arm530_sdk",
            "20",
            "before",
            TRUSTED_SOURCE.to_owned(),
        ),
        (
            after,
            "arm530_sdk_final",
            "10",
            "after",
            private_source_condition(
                "always() && steps.arm530_sdk.outcome != 'skipped' && env.CI296_SOURCE != ''",
            ),
        ),
    ] {
        let step = &steps[at];
        absent(step, &["continue-on-error", "uses"])?;
        require(
            step["id"] == id
                && steps.iter().filter(|s| s["id"] == id).count() == 1
                && step["if"] == condition
                && step["timeout-minutes"] == minutes
                && step["working-directory"] == "${{ env.CI296_SOURCE }}"
                && step["shell"] == "bash"
                && step["env"]
                    == serde_json::json!({"CI296_HOST_CONTROL":"1",
                "ARM530_HELPER_SHA256":"faa58b5a058a927824870ac2df8e9aeed3f33830d62c8bb6407c6e6540277b9b"})
                && step["run"]
                    == format!(
                        "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" {phase}\n"
                    ),
            format!("ARM SDK530 exact {phase} binding changed"),
        )?;
    }
    let run = steps[dependency]["run"]
        .as_str()
        .ok_or("missing ARM SDK530 dependency body")?;
    require(
        run.lines()
            .filter(|line| {
                *line ==
        "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" verify"
            })
            .count()
            == 1
            && !run.lines().any(|line| {
                line.contains("rustup toolchain install") || line.contains("rustup target add")
            }),
        "ARM SDK530 dependencies must verify the sealed SDK without reinstalling it",
    )?;
    Ok(())
}

/// Validate the #296 owned source/alias consumer. Existing private x64
/// diagnostics and the separately admitted Fast schema retain their own gates.
pub fn validate_private_job_layout(job: &Value) -> Result<(), String> {
    let Some(alias) = job["env"]["CI296_ALIAS_BASENAME"].as_str() else {
        return Ok(());
    };
    require(
        alias.starts_with(".ci296-source-") && !alias.contains('/'),
        "invalid private local-action alias",
    )?;
    let steps = job["steps"].as_array().ok_or("missing private job steps")?;
    let native_arm = job["env"]["CI296_NATIVE_ARM"] == "1";
    let carrot_x64_sdk530 = alias == ".ci296-source-rust--carrot-man-runtime" && !native_arm;
    if carrot_x64_sdk530 {
        validate_carrot_x64_sdk530_steps(steps)?;
    }
    let arm_sdk530 = arm_sdk530_role(job);
    if arm_sdk530 {
        validate_arm_sdk530_steps(job, steps)?;
    }
    if native_arm {
        validate_hosted_arm_boundary(job)?;
        require(
            job["env"]["LOCAL_CI_ARCH"] == "${{ matrix.arch }}"
                && job["strategy"]["matrix"]["include"]
                    == serde_json::json!([{"arch":"aarch64","runner":"hosted-arm64"}]),
            "native ARM target matrix differs from its actual host",
        )?;
    }
    let position = |name: &str| {
        steps
            .iter()
            .position(|s| s["name"] == name)
            .ok_or_else(|| format!("missing private stage {name}"))
    };
    let allocate =
        position("Allocate private source cache target and local action alias before checkout")?;
    let checkout = steps
        .iter()
        .position(|s| {
            s["uses"]
                .as_str()
                .unwrap_or("")
                .starts_with("actions/checkout@")
        })
        .ok_or("missing private checkout")?;
    require(
        allocate < checkout,
        "private source allocation must precede checkout",
    )?;
    let stage = steps[allocate]["run"]
        .as_str()
        .ok_or("missing private allocation body")?;
    for marker in [
        "((available >= required))",
        "source_owner=$(mktemp",
        "ln -sT",
        "CI296_ALIAS_IDENTITY=",
        "LOCAL_CI_SOURCE_ROOT=$source",
        "CARGO_TARGET_DIR=$source/rust/target",
        "CARGO_HOME=$job/cargo-home",
        "RUSTUP_HOME=$job/rustup-home",
        "if [[ ${GITHUB_WORKSPACE:-} == %q ]]; then",
    ] {
        require(
            stage.contains(marker),
            format!("private allocation/transport gap: {marker}"),
        )?;
    }
    require(
        stage.find("((available >= required))") < stage.find("source_owner=$(mktemp"),
        "source growth checked after allocation",
    )?;
    require(
        steps[checkout]["with"]["path"] == "${{ env.CI296_CHECKOUT_PATH }}",
        "checkout escaped owned private source",
    )?;
    for key in ["persist-credentials", "set-safe-directory", "clean"] {
        require(
            steps[checkout]["with"][key] == "false",
            format!("unsafe private checkout flag {key}"),
        )?;
    }
    let admission = position("Admit exact private source and index")?;
    let seal = position("Seal private source and index after maintained checks")?;
    require(
        checkout < admission && admission < seal,
        "private admission/seal order changed",
    )?;
    require(
        steps[seal]["id"] == "ci296_source_seal",
        "source seal outcome ID missing",
    )?;
    require(
        steps[seal]["if"] == private_source_condition("always() && env.CI296_SOURCE != ''"),
        "source seal guard changed",
    )?;
    for marker in [
        "cmp \"$CI_JOB_TEMP/proof/index-before.bin\" \"$CI_JOB_TEMP/proof/index-after.bin\"",
        "cmp \"$CI_JOB_TEMP/proof/source-before.sha256\" \"$CI_JOB_TEMP/proof/source-after.sha256\"",
        "git diff --exit-code",
        "git diff --cached --exit-code",
    ] {
        require(
            steps[seal]["run"]
                .as_str()
                .is_some_and(|body| body.contains(marker)),
            "source seal comparison lost",
        )?;
    }
    for step in steps {
        if step["uses"]
            .as_str()
            .is_some_and(|called| called.starts_with("android-actions/setup-android@"))
        {
            require(
                step["env"]["ANDROID_SDK_ROOT"] == "${{ env.CI_JOB_TEMP }}/android-sdk",
                "Android setup escaped owned SDK root",
            )?;
        }
        if let Some(called) = step["uses"].as_str().filter(|s| s.starts_with("./")) {
            require(
                called.starts_with(&format!("./{alias}/source/.github/actions/")),
                "local action escaped same private source",
            )?;
        }
        if step.get("run").is_some() {
            let directory = step["working-directory"]
                .as_str()
                .ok_or("missing private command directory")?;
            let host = directory == "${{ github.workspace }}"
                && step["env"]["CI296_HOST_CONTROL"] == "1"
                && matches!(
                    step["name"].as_str().unwrap_or(""),
                    "Deny fork source and require registered local runner"
                        | "Deny fork source and require GitHub-hosted Linux X64"
                        | "Deny fork source and require GitHub-hosted Ubuntu X64 runner"
                        | "Deny fork source and require GitHub-hosted Linux ARM64"
                        | "Require unavailable Darwin execution interface"
                        | "Prepare GNU and Rust providers on native Darwin"
                        | "Allocate private source cache target and local action alias before checkout"
                        | "Clean only uploaded owned source alias and target"
                );
            let private = directory
                .strip_prefix("${{ env.CI296_SOURCE }}")
                .is_some_and(|suffix| !suffix.split('/').any(|part| part == ".."));
            require(
                host || private,
                "run command escaped private source/owned host boundary",
            )?;
        }
    }
    if job["env"].get("LOCAL_CI_ARCH").is_some() {
        require(
            steps[admission]["id"] == "ci296_source_admission",
            "fresh ARM source admission outcome missing",
        )?;
        require(
            steps[admission]["if"] == TRUSTED_SOURCE,
            "fresh ARM admission guard changed",
        )?;
        let finish = position(if native_arm {
            "Finish native ARM execution evidence"
        } else {
            "Finish emulated architecture evidence"
        })?;
        if native_arm {
            let prepare = position("Prepare native ARM command transport")?;
            require(
                admission < prepare && prepare < finish,
                "native ARM transport preparation order changed",
            )?;
            for (index, mode) in [(prepare, "prepare"), (finish, "finish")] {
                require(
                    steps[index]["run"]
                        == format!("/usr/bin/bash \"$CI296_SOURCE/tools/ci/native-arm\" {mode}\n")
                        && steps[index]["env"]["CI296_HOST_CONTROL"] == "1"
                        && steps[index]["shell"] == "bash"
                        && steps[index]["working-directory"] == "${{ env.CI296_SOURCE }}",
                    "native ARM transport must use the admitted current source helper",
                )?;
                absent(&steps[index], &["continue-on-error"])?;
            }
        }
        require(admission < finish, "ARM finalizer precedes fresh admission")?;
        require(
            steps[finish]["if"]
                == private_source_condition(
                    "always() && steps.ci296_source_admission.outcome == 'success'",
                ),
            "ARM finalizer may load an unadmitted retained alias",
        )?;
        require(
            job["defaults"]["run"]["shell"] == "openpilot-local-shell {0}",
            "ARM-routed job lost its explicit command-file transport",
        )?;
        for step in steps {
            if step.get("run").is_some() && step["env"]["CI296_HOST_CONTROL"] != "1" {
                require(
                    step["shell"] == "openpilot-local-shell {0}",
                    "target command escaped explicit QEMU command-file shell",
                )?;
            }
            if step["uses"].as_str().is_some_and(|called| {
                ["native-json11", "native-eigen", "native-image-sdk"]
                    .iter()
                    .any(|name| called.ends_with(&format!("/.github/actions/{name}")))
            }) {
                require(
                    step["with"]["run-shell"] == "openpilot-local-shell {0}",
                    "ARM native action must receive explicit run-shell input",
                )?;
            }
        }
    }
    let cleanup = &steps[position("Clean only uploaded owned source alias and target")?];
    let condition = cleanup["if"]
        .as_str()
        .ok_or("missing private cleanup condition")?;
    for step in steps {
        if (step["uses"]
            .as_str()
            .unwrap_or("")
            .starts_with("actions/upload-artifact@")
            || step["uses"]
                .as_str()
                .unwrap_or("")
                .ends_with("/.github/actions/native-eigen"))
            && step.get("id").is_some()
        {
            let id = step["id"].as_str().ok_or("invalid upload id")?;
            for suffix in [
                ".outcome == 'success'",
                ".outputs.artifact-id != ''",
                ".outputs.artifact-digest != ''",
            ] {
                require(
                    condition.contains(&format!("steps.{id}{suffix}")),
                    format!("cleanup before authenticated upload {id}"),
                )?;
            }
        }
    }
    // WebRTC failure evidence must survive successful always-run finalizers.
    // All four native producers/checks are required even when receipts upload.
    let mut native_success = Vec::new();
    if alias == ".ci296-source-rust--webrtc-runtime" {
        native_success.extend(["!cancelled()".into(), "success()".into()]);
        let mut previous = admission;
        for (name, id) in [
            (
                "Build current native dependency stager",
                "webrtc_dependency_stager",
            ),
            (
                "Build unchanged original Params and msgq bindings",
                "webrtc_original_bindings",
            ),
            (
                "Compile and check both native service entrypoints",
                "webrtc_native_checks",
            ),
            (
                "Compare real ownership, media, HTTP and failure boundaries",
                "webrtc_real_comparisons",
            ),
        ] {
            let index = position(name)?;
            let stage = &steps[index];
            require(
                previous < index && index < seal,
                "WebRTC required stage order changed",
            )?;
            require(
                stage["id"] == id && steps.iter().filter(|s| s["id"] == id).count() == 1,
                format!("WebRTC required outcome ID missing or duplicated: {id}"),
            )?;
            require(
                stage.get("continue-on-error").is_none(),
                "WebRTC required stage cannot continue on error",
            )?;
            require(
                stage["if"] == TRUSTED_SOURCE,
                "WebRTC required stage lost normal trusted success gate",
            )?;
            native_success.push(format!("steps.{id}.outcome == 'success'"));
            previous = index;
        }
    }
    let mut expected = private_upload_conditions(steps)?;
    expected.splice(1..1, native_success.iter().cloned());
    expected.push("steps.ci296_source_seal.outcome == 'success'".into());
    if alias == ".ci296-source-tests--build_release" {
        let generated =
            position("Check actual generated asset consumers and unchanged output algorithms")?;
        require(
            generated < seal
                && steps[generated]["id"] == "generated_assets_check"
                && steps
                    .iter()
                    .filter(|step| step["id"] == "generated_assets_check")
                    .count()
                    == 1
                && steps[generated]["if"] == TRUSTED_SOURCE,
            "generated asset cleanup needs the bound real consumer outcome",
        )?;
        absent(&steps[generated], &["continue-on-error"])?;
        expected.push("steps.generated_assets_check.outcome == 'success'".into());
    }
    if alias == ".ci296-source-user-docs--check_user_docs" {
        let before = position("Prepare private Rust 1.94 SDK bound to the private docs source")?;
        let after = position("Verify prepared Rust SDK after all attempted docs commands")?;
        let recovery = position("Require owned docs groups to be non-live before cleanup")?;
        let upload = position("Retain private job ownership and source evidence")?;
        require(
            admission < before
                && before < position("Check code and user documentation changes")?
                && position("Check code and user documentation changes")?
                    < position("Test user documentation checker")?
                && position("Test user documentation checker")? < seal
                && seal < after
                && after < recovery
                && recovery < upload,
            "docs SDK/recovery stage order changed",
        )?;
        for (at, id, condition) in [
            (before, "prepared_rust", TRUSTED_SOURCE.to_owned()),
            (
                after,
                "prepared_rust_final",
                private_source_condition("always() && steps.prepared_rust.outcome == 'success'"),
            ),
            (
                recovery,
                "ci296_docs_recovery",
                private_source_condition("always() && env.CI296_SOURCE != ''"),
            ),
        ] {
            require(
                steps[at]["id"] == id && steps.iter().filter(|step| step["id"] == id).count() == 1,
                format!("docs safety stage ID missing or duplicated: {id}"),
            )?;
            require(
                steps[at]["if"] == condition,
                format!("docs safety guard changed: {id}"),
            )?;
            absent(&steps[at], &["continue-on-error"])?;
        }
        for (at, phase) in [(before, "before"), (after, "after")] {
            let command = format!(
                "DIAG_SOURCE=\"$CI296_SOURCE\" \"$CI296_SOURCE/rust/tools/prepare-readonly-ci-rust\" {phase}"
            );
            require(
                steps[at]["run"]
                    .as_str()
                    .is_some_and(|body| body.lines().any(|line| line == command)),
                format!("docs SDK {phase} must bind the same private source"),
            )?;
        }
        let recover = steps[recovery]["run"]
            .as_str()
            .ok_or("missing docs recovery body")?;
        for marker in [
            "for phase in mapped user-docs-tests; do",
            "test -f \"$receipt\" && test ! -L \"$receipt\"",
            "grep -Fx \"phase=$phase\" \"$receipt\"",
            "grep -Fx \"source=$GITHUB_SHA\" \"$receipt\"",
            "grep -Fx \"owner=$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB\" \"$receipt\"",
            "grep -Fx 'recovery=pass' \"$receipt\"",
        ] {
            require(
                recover.contains(marker),
                format!("docs recovery receipt binding missing: {marker}"),
            )?;
        }
        expected.extend([
            "steps.prepared_rust_final.outcome == 'success'".into(),
            "steps.ci296_docs_recovery.outcome == 'success'".into(),
        ]);
    }
    if alias == ".ci296-source-rust--dbc-darwin" {
        let sdk = position("Seal native Darwin providers and private SDK after maintained checks")?;
        let upload = position("Preserve macOS host and disk evidence")?;
        require(
            seal < sdk && sdk < upload,
            "Darwin SDK seal must precede evidence upload",
        )?;
        require(
            steps[sdk]["id"] == "ci296_mac_sdk_seal"
                && steps
                    .iter()
                    .filter(|step| step["id"] == "ci296_mac_sdk_seal")
                    .count()
                    == 1,
            "Darwin SDK seal ID missing or duplicated",
        )?;
        require(
            steps[sdk]["if"] == private_source_condition("always() && env.CI296_SOURCE != ''"),
            "Darwin SDK seal lost failure finalizer",
        )?;
        absent(&steps[sdk], &["continue-on-error"])?;
        let body = steps[sdk]["run"]
            .as_str()
            .ok_or("missing Darwin SDK seal body")?;
        for marker in [
            "test -s \"$proof/providers-before.txt\"",
            "cmp \"$proof/providers-before.txt\" \"$proof/providers-after.txt\"",
            "for kind in rust mac; do",
            "cmp \"$proof/$kind-sdk-identity.txt\" \"$proof/$kind-sdk-identity-after.txt\"",
            "cmp \"$proof/$kind-sdk-before.sha256\" \"$proof/$kind-sdk-after.sha256\"",
            "cmp \"$proof/mac-sdk-version.txt\" \"$proof/mac-sdk-version-after.txt\"",
        ] {
            require(
                body.contains(marker),
                format!("Darwin SDK seal binding missing: {marker}"),
            )?;
        }
        expected.push("steps.ci296_mac_sdk_seal.outcome == 'success'".into());
    }
    if arm_sdk530 {
        expected.push("steps.arm530_sdk_final.outcome == 'success'".into());
    }
    if carrot_x64_sdk530 {
        expected.push("steps.carrot530_sdk_final.outcome == 'success'".into());
    }
    expected.push("env.CI296_SOURCE_OWNER != ''".into());
    require(
        condition == private_source_condition(&expected.join(" && ")),
        "source cleanup requires the exact successful seal and upload receipts",
    )?;
    let common = &steps[position("Clean common owned temporary paths after uploaded source")?];
    let mut expected = private_upload_conditions(steps)?;
    expected.splice(1..1, native_success.iter().cloned());
    if arm_sdk530 {
        expected.push("steps.arm530_sdk_final.outcome == 'success'".into());
    }
    if carrot_x64_sdk530 {
        expected.push("steps.carrot530_sdk_final.outcome == 'success'".into());
    }
    expected.push("steps.ci296_source_cleanup.outcome == 'success'".into());
    require(
        common["if"] == private_source_condition(&expected.join(" && ")),
        "common cleanup bypasses retained source/proof hold",
    )?;
    let cleanup = cleanup["run"]
        .as_str()
        .ok_or("missing private cleanup body")?;
    for marker in [
        "UPLOAD_ID =~",
        "UPLOAD_DIGEST =~",
        "stat -c '%d:%i'",
        ".ci296-owner",
        "readlink --",
        "CI296_ALIAS_IDENTITY",
        "test ! -L",
        "rm -- \"$alias_path\"",
    ] {
        require(
            cleanup.contains(marker),
            format!("private cleanup guard missing {marker}"),
        )?;
    }
    Ok(())
}
/// Preserve pending as well as running jobs. GitHub defaults to replacing the
/// pending job even when cancel-in-progress is false; queue: max opts into the
/// supported queue (up to 100 pending jobs) without changing concurrency groups.
pub fn validate_concurrency(workflow: &Value) -> Result<(), String> {
    let check = |scope: &Value| -> Result<(), String> {
        if let Some(concurrency) = scope.get("concurrency") {
            require(
                concurrency["cancel-in-progress"] == "false",
                "concurrency must preserve running jobs",
            )?;
            require(
                concurrency["queue"] == "max",
                "concurrency must retain pending jobs with queue: max",
            )?;
        }
        Ok(())
    };
    check(workflow)?;
    if let Some(jobs) = workflow["jobs"].as_object() {
        for job in jobs.values() {
            check(job)?;
        }
    }
    Ok(())
}

/// 승인한 legacy/X64 label과 명시적 실제 호스트 경계를 가진 표준 ARM label을 검증한다.
/// matrix/expression/unknown runner fallback은 허용하지 않는다.
pub fn validate_local_routing(workflow: &Value) -> Result<(), String> {
    validate_concurrency(workflow)?;
    require(
        workflow["on"].get("pull_request_target").is_none(),
        "pull_request_target cannot execute source on persistent runner",
    )?;
    let jobs = workflow["jobs"]
        .as_object()
        .ok_or("missing local workflow jobs")?;
    for (name, job) in jobs {
        if let Some(called) = job["uses"].as_str() {
            require(
                called.starts_with("./.github/workflows/"),
                "external reusable runner routing is not admitted",
            )?;
            continue;
        }
        let darwin = name == "dbc-darwin";
        let native_arm = job["env"]["CI296_NATIVE_ARM"] == "1";
        if native_arm {
            validate_hosted_arm_boundary(job)?;
        }
        require(
            if darwin {
                job["runs-on"].as_str() == Some(DARWIN_RUNNER)
            } else if native_arm {
                job["runs-on"].as_str() == Some(HOSTED_ARM_RUNNER)
            } else {
                approved_runner(&job["runs-on"])
            },
            format!("{name}: exact approved platform required; no matrix or platform fallback"),
        )?;
        if darwin {
            require(
                job["env"]["CI296_ALIAS_BASENAME"] == ".ci296-source-rust--dbc-darwin",
                "Darwin source alias differs",
            )?;
            let boundary = job["steps"][0]["run"]
                .as_str()
                .ok_or("missing native Darwin boundary")?;
            let guards = [
                "test \"$GITHUB_ACTIONS\" = true",
                "test \"$GITHUB_REPOSITORY\" = bin9208/openpilot-rust",
                "test \"$RUNNER_ENVIRONMENT\" = github-hosted",
                "test \"$RUNNER_OS\" = macOS",
                "test \"$RUNNER_ARCH\" = ARM64",
                "test \"$(uname -s)\" = Darwin",
                "test \"$(uname -m)\" = arm64",
            ];
            let prelude = format!("{FORK_DENIAL}{}\n", guards.join("\n"));
            let body = boundary
                .strip_prefix("set -euo pipefail\n")
                .unwrap_or(boundary);
            require(
                body.starts_with(&prelude),
                "native Darwin physical guards must execute immediately after fork refusal",
            )?;
        }
        validate_private_job_layout(job)?;
        let steps = job["steps"]
            .as_array()
            .ok_or("missing source trust boundary")?;
        source_result_binding(steps.first().ok_or("missing fork refusal step")?)?;
        if source_boundary_name(steps[0]["name"].as_str().unwrap_or("")) {
            require(
                steps[0].get("if").is_none(),
                "fork refusal must not be skipped",
            )?;
            for step in &steps[1..] {
                let condition = step["if"]
                    .as_str()
                    .ok_or("source-reading step lacks trust condition")?;
                require(trusted_condition(condition), "untrusted source step bypass")?;
            }
        } else {
            require(
                job["if"] == "${{ always() }}" && steps.len() == 1,
                "result aggregate must still run and explicitly reject forks",
            )?;
        }
    }
    Ok(())
}

pub fn check_workflow_policy(files: &[(String, String)]) -> Result<(), String> {
    let workflows = files
        .iter()
        .map(|(name, text)| parse(text).map(|v| (name.as_str(), v)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    for workflow in workflows.values() {
        validate_local_routing(workflow)?;
    }
    let get = |name: &str| {
        workflows
            .get(name)
            .ok_or_else(|| format!("missing workflow {name}"))
    };
    require(
        !workflows.contains_key("fast-checks.yml"),
        "Fast checks workflow has been retired",
    )?;
    let integration = get("integration.yml")?;
    for event in ["push", "pull_request"] {
        require(
            integration["on"][event] == serde_json::json!({"branches":["dev","main"]}),
            format!("integration {event} routing changed"),
        )?;
    }
    require(
        integration["jobs"]["build"]["with"]["run_number"] == "1",
        "integration must run a build",
    )?;
    let gate = &integration["jobs"]["gate"];
    require(
        strings(&gate["needs"])? == expected_strings(&["build", "navigation", "apk", "jetlink"]),
        "integration gate dependencies changed",
    )?;
    require(
        gate["if"] == "${{ always() }}",
        "integration gate must always run",
    )?;
    let script = gate["steps"][0]["run"]
        .as_str()
        .ok_or("missing integration gate script")?;
    for name in ["BUILD", "NAVIGATION", "APK", "JETLINK"] {
        require(
            script.contains(&format!("test \"${name}\" = success")),
            format!("integration gate no longer requires {name}"),
        )?;
    }
    absent(gate, &["continue-on-error"])?;
    let gate_steps = gate["steps"]
        .as_array()
        .ok_or("missing integration result step")?;
    require(
        gate_steps.len() == 1,
        "integration aggregate must retain one mandatory result step",
    )?;
    let step = &gate_steps[0];
    source_result_binding(step)?;
    let bindings = [
        ("BUILD", "build"),
        ("NAVIGATION", "navigation"),
        ("APK", "apk"),
        ("JETLINK", "jetlink"),
    ];
    let environment = step["env"]
        .as_object()
        .ok_or("missing integration bindings")?;
    require(
        environment.len() == bindings.len() + 1,
        "integration binding count changed",
    )?;
    for (variable, worker) in bindings {
        require(
            step["env"][variable] == format!("${{{{ needs.{worker}.result }}}}"),
            format!("integration binding changed: {variable}"),
        )?;
    }
    let mut success = bindings
        .iter()
        .map(|(name, _)| ((*name).into(), "success".into()))
        .collect::<BTreeMap<_, _>>();
    success.insert("SOURCE_ALLOWED".into(), "true".into());
    require(
        run_shell(script, &success)
            .map_err(|e| e.to_string())?
            .success(),
        "integration rejects successful workers",
    )?;
    for variable in ["BUILD", "NAVIGATION", "APK", "JETLINK", "SOURCE_ALLOWED"] {
        for state in [
            None,
            Some(""),
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some("false"),
        ] {
            let mut values = success.clone();
            if let Some(state) = state {
                values.insert(variable.into(), state.into());
            } else {
                values.remove(variable);
            }
            require(
                !run_shell(script, &values)
                    .map_err(|e| e.to_string())?
                    .success(),
                format!("integration accepts {variable}={state:?}"),
            )?;
        }
    }
    for name in ["build", "navigation", "apk", "jetlink"] {
        let job = &integration["jobs"][name];
        absent(job, &["if", "continue-on-error"])?;
        let called = job["uses"]
            .as_str()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|p| p.to_str())
            .ok_or("missing reusable workflow")?;
        require(
            get(called)?["on"].get("workflow_call").is_some(),
            format!("{called} is not reusable"),
        )?;
    }
    let rust = get("rust.yml")?;
    validate_native_arm_reuse(rust, get("rust-native-arm.yml")?)?;
    require(
        rust["jobs"]["dbc-darwin"]["uses"] == "./.github/workflows/rust-dbc-darwin.yml",
        "Darwin dependency must call its same-source native workflow",
    )?;
    absent(&rust["jobs"]["dbc-darwin"], &["if", "continue-on-error"])?;
    let darwin = get("rust-dbc-darwin.yml")?;
    require(
        darwin["on"].get("workflow_call").is_some()
            && darwin["jobs"]
                .as_object()
                .is_some_and(|jobs| jobs.len() == 1 && jobs.contains_key("dbc-darwin")),
        "native Darwin workflow must retain one required reusable job",
    )?;
    let docs = get("user-docs.yaml")?;
    require(
        expected_strings(&["dev", "main"]).is_subset(&strings(&docs["on"]["push"]["branches"])?),
        "user docs protected push routing changed",
    )?;
    require(
        docs["on"].get("pull_request").is_some(),
        "user docs PR routing missing",
    )?;
    Ok(())
}

/// Run the actual workflow shell in precisely the supplied environment. Capture
/// deadlines/group ownership prevent a malformed gate from hanging the CI test.
pub fn run_shell(script: &str, environment: &BTreeMap<String, String>) -> io::Result<ExitStatus> {
    let mut argv = vec!["env".into(), "-i".into()];
    argv.extend(
        environment
            .iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    argv.extend([
        "bash".into(),
        "--noprofile".into(),
        "--norc".into(),
        "-eo".into(),
        "pipefail".into(),
        "-c".into(),
        script.into(),
    ]);
    let spec = CommandSpec::new(argv, Duration::from_secs(5));
    let output = NativeRunner
        .run(&spec)
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(ExitStatus::from_raw(if output.code >= 0 {
        output.code << 8
    } else {
        -output.code
    }))
}

pub const DIAGNOSTIC_LOCAL_RUNNER: [&str; 4] = ["self-hosted", "Linux", "X64", "dev-md-local"];
pub const DIAGNOSTIC_TRUSTED_SOURCE: &str = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}";
pub const DIAGNOSTIC_FORK_DENIAL: &str = "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied on persistent bin-actions' >&2; exit 1; }\n";
pub const DIAGNOSTIC_PROOF: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && env.CI_JOB_TEMP != '') }}";
pub const DIAGNOSTIC_CLEANUP: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.fast_evidence.outcome == 'success' && steps.fast_evidence.outputs.artifact-id != '' && steps.fast_evidence.outputs.artifact-digest != '' && env.DIAG_SOURCE_OWNER != '' && steps.source_final.outcome == 'success' && steps.prepared_rust_final.outcome == 'success') }}";
pub const DIAGNOSTIC_COMMON_CLEANUP: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.fast_evidence.outcome == 'success' && steps.fast_evidence.outputs.artifact-id != '' && steps.fast_evidence.outputs.artifact-digest != '' && steps.fast_source_cleanup.outcome == 'success') }}";

/// Preserve the required fast-check status while its six mandatory workers run
/// independently. Execute the aggregate shell to reject accidental fail-open
/// edits, including absent results that GitHub exposes as empty strings.
pub fn validate_fast_gate(workflow: &Value) -> Result<(), String> {
    let opt_in = [
        "OPENPILOT_CI_PREPARED_RUST_ROOT",
        "OPENPILOT_CI_PREPARED_RUST_MANIFEST",
    ];
    absent(&workflow["env"], &opt_in)?;
    require(
        workflow["concurrency"]["cancel-in-progress"] == "false",
        "Fast must not cancel an existing run",
    )?;
    require(
        workflow["concurrency"]["queue"] == "max",
        "Fast must retain pending runs rather than replace them",
    )?;
    let bindings = [
        ("NATIVE", "native"),
        ("NATIVE_TOOLS", "native-tools"),
        ("NATIVE_CORPUS", "native-corpus"),
        ("NATIVE_TIMING", "native-timing"),
        ("NATIVE_ALERTS", "native-alerts"),
        ("NATIVE_ALERT_MANAGER", "native-alert-manager"),
        ("PYTHON_ANDROID", "python-android"),
    ];
    let jobs = workflow["jobs"].as_object().ok_or("missing fast jobs")?;
    for (_, name) in bindings {
        let worker = jobs
            .get(name)
            .ok_or_else(|| format!("missing fast worker {name}"))?;
        absent(&worker["env"], &opt_in)?;
        absent(worker, &["if", "continue-on-error", "needs"])?;
        require(
            worker["timeout-minutes"] == "7",
            format!("fast worker {name} must retain its seven-minute bound"),
        )?;
        let steps = worker["steps"]
            .as_array()
            .ok_or("missing fast worker steps")?;
        require(!steps.is_empty(), "fast worker has no checks")?;
        require(
            approved_runner(&worker["runs-on"]),
            "Fast workers require exact legacy labels or ubuntu-24.04",
        )?;
        let boundary = &steps[0];
        absent(boundary, &["if", "continue-on-error"])?;
        require(
            boundary["env"]["SOURCE_ALLOWED"] == DIAGNOSTIC_TRUSTED_SOURCE,
            "missing Fast fork source binding",
        )?;
        require(
            boundary["run"]
                .as_str()
                .is_some_and(|s| s.starts_with(DIAGNOSTIC_FORK_DENIAL)),
            "Fast fork refusal must precede source execution",
        )?;
        let prepared_final = if name == "native" {
            "Verify provisioned Rust SDK contents and identities after routing"
        } else {
            "Verify read-only Rust SDK contents and identities after maintained checks"
        };
        // 모든 consumer에서 after/cleanup 조건을 유일한 실제 생산 단계에 결합한다.
        let mut stages = vec![
            (
                "Admit provisioned Rust without installing a second toolchain",
                "prepared_rust",
            ),
            (prepared_final, "prepared_rust_final"),
            (
                "Seal Fast source and index after all maintained checks",
                "source_final",
            ),
            ("Retain Fast job ownership evidence", "fast_evidence"),
            (
                "Clean only uploaded private Fast source",
                "fast_source_cleanup",
            ),
        ];
        if name == "native" {
            // SDK 권한 준비는 기존 routing worker만 수행한다.
            stages.extend([
                (
                    "Remove shared write permission from the owned provisioned SDK",
                    "sdk_permissions",
                ),
                (
                    "Verify SDK contents after permission preparation even on failure",
                    "sdk_permissions_final",
                ),
            ]);
        }
        for (stage, id) in stages {
            require(
                steps
                    .iter()
                    .filter(|step| step["name"] == stage && step["id"] == id)
                    .count()
                    == 1
                    && steps.iter().filter(|step| step["id"] == id).count() == 1
                    && steps.iter().filter(|step| step["name"] == stage).count() == 1,
                format!("Fast {name} outcome stage missing or duplicated: {id}"),
            )?;
        }
        let admit = steps
            .iter()
            .position(|step| step["id"] == "prepared_rust")
            .ok_or("missing prepared SDK admission")?;
        require(steps.len() >= 5, "missing Fast final stages")?;
        let final_start = steps.len() - 5;
        for (step, stage) in steps[final_start..].iter().zip([
            prepared_final,
            "Seal Fast source and index after all maintained checks",
            "Retain Fast job ownership evidence",
            "Clean only uploaded private Fast source",
            "Clean common owned Fast job temporary paths",
        ]) {
            require(step["name"] == stage, "Fast final stage order changed")?;
        }
        require(admit < final_start, "SDK admission must precede checks")?;
        if name != "native" {
            require(
                !steps.iter().any(|step| {
                    step["id"] == "sdk_permissions" || step["id"] == "sdk_permissions_final"
                }),
                "SDK permission preparation belongs only to native routing",
            )?;
        } else {
            let permissions = steps
                .iter()
                .position(|step| step["id"] == "sdk_permissions")
                .unwrap();
            let permissions_final = steps
                .iter()
                .position(|step| step["id"] == "sdk_permissions_final")
                .unwrap();
            require(
                admit < permissions
                    && permissions < permissions_final
                    && permissions_final < final_start,
                "private SDK preparation must precede permission verification",
            )?;
        }
        for (index, phase) in [(admit, "before"), (final_start, "after")] {
            let command =
                format!("\"$GITHUB_WORKSPACE/rust/tools/prepare-readonly-ci-rust\" {phase}");
            require(
                steps[index]["run"]
                    .as_str()
                    .is_some_and(|s| s.trim() == command)
                    && steps[index]["shell"] == "bash"
                    && steps[index]["working-directory"] == "${{ env.DIAG_SOURCE }}",
                "Fast read-only SDK stage must execute its bound helper",
            )?;
        }
        for (index, step) in steps.iter().enumerate() {
            absent(&step["env"], &opt_in)?;
            let script = step["run"].as_str().unwrap_or("");
            if script.contains("/rust/tools/check-disk-space\"") {
                let commands = script.replace("\\\n", " ");
                let inline = commands
                    .lines()
                    .filter(|line| line.contains("/rust/tools/check-disk-space\""))
                    .all(|line| line.trim_start().starts_with(
                        "OPENPILOT_CI_PREPARED_RUST_ROOT=\"$CI_PREPARED_RUST_ROOT\" OPENPILOT_CI_PREPARED_RUST_MANIFEST=\"$CI_PREPARED_RUST_MANIFEST\" ",
                    ));
                require(
                    admit < index && index < final_start && inline,
                    "Fast disk bootstrap requires inline prepared SDK admission",
                )?;
            }
        }
        for step in &steps[1..] {
            absent(step, &["continue-on-error"])?;
            let condition = match (name, step["name"].as_str().unwrap_or("")) {
                ("native", "Verify SDK contents after permission preparation even on failure") => {
                    private_source_condition(
                        "always() && steps.sdk_permissions.outcome != 'skipped' && env.DIAG_SOURCE != ''",
                    )
                }
                (_, stage) if stage == prepared_final => {
                    private_source_condition("always() && steps.prepared_rust.outcome == 'success'")
                }
                (_, "Seal Fast source and index after all maintained checks") => {
                    private_source_condition("always() && env.DIAG_SOURCE != ''")
                }
                ("native", "Clean only uploaded private Fast source") => private_source_condition(
                    "always() && steps.fast_evidence.outcome == 'success' && steps.fast_evidence.outputs.artifact-id != '' && steps.fast_evidence.outputs.artifact-digest != '' && env.DIAG_SOURCE_OWNER != '' && steps.source_final.outcome == 'success' && steps.prepared_rust_final.outcome == 'success' && steps.sdk_permissions_final.outcome == 'success'",
                ),
                (_, "Retain Fast job ownership evidence") => DIAGNOSTIC_PROOF.to_owned(),
                (_, "Clean only uploaded private Fast source") => DIAGNOSTIC_CLEANUP.to_owned(),
                (_, "Clean common owned Fast job temporary paths") => {
                    DIAGNOSTIC_COMMON_CLEANUP.to_owned()
                }
                _ => DIAGNOSTIC_TRUSTED_SOURCE.to_owned(),
            };
            require(
                step["if"] == condition,
                "mandatory trusted Fast step condition changed",
            )?;
        }
    }
    let gate = jobs.get("fast").ok_or("missing fast aggregate")?;
    absent(gate, &["continue-on-error"])?;
    require(
        approved_runner(&gate["runs-on"]),
        "Fast aggregate requires exact legacy labels or ubuntu-24.04",
    )?;
    require(
        gate["name"] == "fast checks",
        "required fast check name changed",
    )?;
    require(
        gate["if"] == "${{ always() }}",
        "fast aggregate must always run",
    )?;
    require(
        strings(&gate["needs"])?
            == expected_strings(&[
                "native",
                "native-tools",
                "native-corpus",
                "native-timing",
                "native-alerts",
                "native-alert-manager",
                "python-android",
            ]),
        "fast aggregate dependencies changed",
    )?;
    let steps = gate["steps"]
        .as_array()
        .ok_or("missing fast aggregate steps")?;
    require(
        steps.len() == 1,
        "fast aggregate must have one mandatory result check",
    )?;
    let step = &steps[0];
    absent(step, &["if", "continue-on-error"])?;
    let environment = step["env"]
        .as_object()
        .ok_or("missing fast result bindings")?;
    require(
        environment.len() == bindings.len() + 1,
        "fast result binding count changed",
    )?;
    for (variable, job) in bindings {
        require(
            environment.get(variable)
                == Some(&Value::String(format!("${{{{ needs.{job}.result }}}}"))),
            format!("fast result binding changed: {variable}"),
        )?;
    }
    let script = step["run"]
        .as_str()
        .ok_or("missing fast aggregate script")?;
    require(
        step["env"]["SOURCE_ALLOWED"] == DIAGNOSTIC_TRUSTED_SOURCE,
        "missing Fast aggregate source trust",
    )?;
    require(
        script.starts_with(DIAGNOSTIC_FORK_DENIAL),
        "Fast aggregate must explicitly refuse forks",
    )?;
    let mut success = bindings
        .iter()
        .map(|(name, _)| ((*name).into(), "success".into()))
        .collect::<BTreeMap<_, _>>();
    success.insert("SOURCE_ALLOWED".into(), "true".into());
    for state in [None, Some(""), Some("false"), Some("skipped")] {
        let mut denied = success.clone();
        if let Some(state) = state {
            denied.insert("SOURCE_ALLOWED".into(), state.into());
        } else {
            denied.remove("SOURCE_ALLOWED");
        }
        require(
            !run_shell(script, &denied)
                .map_err(|e| e.to_string())?
                .success(),
            "Fast fork aggregate passed a skip",
        )?;
    }
    require(
        run_shell(script, &success)
            .map_err(|e| e.to_string())?
            .success(),
        "fast aggregate rejects successful workers",
    )?;
    for (variable, _) in bindings {
        for result in [
            Some("failure"),
            Some("cancelled"),
            Some("skipped"),
            Some("timed_out"),
            Some(""),
            None,
        ] {
            let mut values = success.clone();
            if let Some(result) = result {
                values.insert(variable.into(), result.into());
            } else {
                values.remove(variable);
            }
            require(
                !run_shell(script, &values)
                    .map_err(|e| e.to_string())?
                    .success(),
                format!("fast aggregate accepts {variable}={result:?}"),
            )?;
        }
    }
    Ok(())
}

pub fn process_alive_from(read: impl FnOnce() -> io::Result<String>) -> io::Result<bool> {
    match read() {
        Ok(text) => text
            .split_once(')')
            .and_then(|(_, fields)| fields.split_whitespace().next())
            .map(|state| state != "Z")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing process state")),
        Err(error) if matches!(error.raw_os_error(), Some(2 | 3)) => Ok(false),
        Err(error) => Err(error),
    }
}

// Independent expected mapping retained from the original CI isolation tests.
const RUST_GATE_BINDINGS: &[(&str, &str)] = &[
    ("WORKSPACE", "workspace"),
    ("LLVM20", "model-llvm20"),
    ("MEMORY", "model-memory"),
    ("PIPELINES", "model-pipelines"),
    ("MODEL_FAMILY", "model-family-retirement"),
    ("LOGGER", "logger-runtime"),
    ("SUPPORT", "support-runtime"),
    ("TELEMETRY", "telemetry-runtime"),
    ("STARTUP", "startup-runtime"),
    ("HARDWARE", "hardware-runtime"),
    ("PLATFORM", "platform-runtime"),
    ("STARTUP_SERVICES", "startup-services"),
    ("SENSOR_AUDIO", "sensor-audio"),
    ("GNSS", "gnss-runtime"),
    ("ESTIMATION", "estimation-runtime"),
    ("UI_CONNECTIVITY", "ui-connectivity"),
    ("ATHENA", "athena-runtime"),
    ("CONTROLS", "controls-runtime"),
    ("UPLOAD_TIMEOUTS", "web-upload-timeouts"),
    ("JOYSTICKD", "joystickd-runtime"),
    ("PLANNER", "planner-runtime"),
    ("PLANNER_MEMORY", "planner-memory"),
    ("CARD", "card-runtime"),
    ("RADAR", "radar-runtime"),
    ("RADAR_ARM", "radar-arm"),
    ("RADAR_MEMORY", "radar-memory"),
    ("NAVD", "navd-runtime"),
    ("RADARD", "radard-runtime"),
    ("CARROT_NAVI", "carrot-navi-runtime"),
    ("CARROT_NAVI_ARM", "carrot-navi-arm"),
    ("SELFDRIVE", "selfdrive-runtime"),
    ("PANDA", "panda-runtime"),
    ("CARROT_MAN", "carrot-man-runtime"),
    ("WEBRTC", "webrtc-runtime"),
    ("WEBCAM", "webcam-runtime"),
    ("DBC_DARWIN", "dbc-darwin"),
    ("CAMERA", "camera-runtime"),
    ("ENCODER", "encoder-runtime"),
    ("XIAOGE", "xiaoge-runtime"),
    ("XIAOGE_MEMORY", "xiaoge-memory"),
    ("UI_RUNTIME", "ui-runtime"),
    ("NATIVE_ARM", "native-arm"),
];

/// 최초 3개 native ARM scope와 main의 동일-source caller를 결합한다.
pub const NATIVE_ARM_BINDINGS: &[(&str, &str)] = &[
    ("planner-runtime", "PLANNER"),
    ("carrot-man-runtime", "CARROT_MAN"),
    ("webrtc-runtime", "WEBRTC"),
    ("webcam-runtime", "WEBCAM"),
    ("xiaoge-runtime", "XIAOGE"),
    ("radard-runtime", "RADARD"),
    ("radar-arm", "RADAR_ARM"),
    ("carrot-navi-arm", "CARROT_NAVI_ARM"),
];

pub fn validate_native_arm_reuse(main: &Value, native: &Value) -> Result<(), String> {
    require(
        main["jobs"]["native-arm"]
            == serde_json::json!({"uses":"./.github/workflows/rust-native-arm.yml"}),
        "native ARM requires its unconditional same-source caller",
    )?;
    require(
        native["on"] == serde_json::json!({"workflow_call":{}})
            && native["permissions"] == serde_json::json!({"contents":"read"})
            && native["env"] == serde_json::json!({"RUSTUP_TOOLCHAIN":"1.94.0"})
            && native["concurrency"]
                == serde_json::json!({
                    "group":"rust-native-arm-${{ github.event_name }}-${{ github.ref }}",
                    "cancel-in-progress":"false", "queue":"max"
                }),
        "native ARM reusable trigger, permissions or toolchain changed",
    )?;
    // Fast의 source policy에서도 41개 기존 결과와 새 ARM 결과의 결합을 확인한다.
    // 실제 shell의 음성 실행은 validate_rust_gate와 isolation tests가 담당한다.
    let main_gate = &main["jobs"]["fast"];
    let main_steps = main_gate["steps"]
        .as_array()
        .ok_or("missing main Rust result step")?;
    require(
        main_steps.len() == 1 && main_gate["if"] == "${{ always() }}",
        "main Rust result gate changed",
    )?;
    let main_step = &main_steps[0];
    source_result_binding(main_step)?;
    let main_script = main_step["run"]
        .as_str()
        .and_then(|s| s.strip_prefix(FORK_DENIAL))
        .ok_or("missing main Rust result checks")?;
    require(
        strings(&main_gate["needs"])? == RUST_GATE_BINDINGS.iter().map(|(_, job)| *job).collect()
            && main_gate["needs"]
                .as_array()
                .is_some_and(|v| v.len() == RUST_GATE_BINDINGS.len())
            && main_step["env"]
                .as_object()
                .is_some_and(|v| v.len() == RUST_GATE_BINDINGS.len() + 1)
            && main_script.lines().count() == RUST_GATE_BINDINGS.len(),
        "main Rust must require all 42 results exactly once",
    )?;
    for (variable, job) in RUST_GATE_BINDINGS {
        let expected = format!("test \"${variable}\" = success");
        require(
            main_step["env"][*variable] == format!("${{{{ needs.{job}.result }}}}")
                && main_script.lines().filter(|line| *line == expected).count() == 1,
            format!("main Rust result binding/check changed: {variable}"),
        )?;
    }
    let jobs = native["jobs"]
        .as_object()
        .ok_or("missing native ARM jobs")?;
    let leaves = NATIVE_ARM_BINDINGS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    require(
        jobs.len() == leaves.len() + 1
            && leaves.iter().all(|name| jobs.contains_key(*name))
            && jobs.contains_key("native-arm-checks"),
        "native ARM required leaf roster changed",
    )?;
    validate_local_routing(native)?;
    for (name, timeout, stages) in [
        (
            "planner-runtime",
            "90",
            &[
                "Reserve capacity and install original planner dependencies",
                "Build pinned solvers and unchanged original bindings",
                "Compile and check the complete native planner",
                "Compare exact policies, main loops, real IPC and lifecycle",
                "Check generated C with ASan, UBSan and allocator leak detection",
            ][..],
        ),
        (
            "carrot-man-runtime",
            "60",
            &[
                "Reserve runner capacity and install pinned dependencies",
                "Stage exact optional GEOS provider",
                "Build owner and unchanged source bindings",
                "Compare affected original policy, cereal and time boundaries",
                "Compare actual owned Params, IPC, sockets and upload recipients",
            ][..],
        ),
        (
            "webrtc-runtime",
            "90",
            &[
                "Reserve capacity and install pinned source and native providers",
                "Build unchanged original Params and msgq bindings",
                "Compile and check both native service entrypoints",
                "Compare real ownership, media, HTTP and failure boundaries",
            ][..],
        ),
        (
            "webcam-runtime",
            "90",
            &[
                "Install exact source closure and native capture providers",
                "Build pinned hardened videoio and unchanged source IPC bindings",
                "Build native daemon and check all webcam targets",
                "Compare actual pixels, independent workers, IPC and Linux entrypoints",
            ][..],
        ),
        (
            "xiaoge-runtime",
            "75",
            &[
                "Admit pinned LFS models before source baseline",
                "Reserve capacity and install Xiaoge build prerequisites",
                "Retrieve the unchanged original Xiaoge models",
                "Compile pinned OpenCV and original Python bindings",
                "Compile and check all native Xiaoge targets",
                "Compare source pixels, model results, sockets, live IPC and lifecycle",
            ][..],
        ),
        (
            "radard-runtime",
            "90",
            &[
                "Reserve capacity and install original oracle dependencies",
                "Build unchanged native Params and msgq bindings",
                "Require actual native imports and declared source helper imports",
                "Require Rust types, tests, complete build and Clippy",
                "Compare existing source recordings and all Rust policy snapshots",
                "Check radard IPC preconditioning input contract",
                "Compare exact availability deadline with original state",
                "Compare independent actual IPC and signal lifecycle",
                "Preserve bounded final binaries and their identities",
            ][..],
        ),
        (
            "radar-arm",
            "90",
            &[
                "Configure original Radar IPC imports",
                "Reserve runner capacity and install generic ARM dependencies",
                "Build unchanged native ARM bindings and generate radar assets",
                "Stage pinned generic ARM numerical dependency",
                "Build and execute generic ARM RadarCAN tests and artifacts",
                "Require exact RadarCAN source comparison on native ARM",
                "Require actual original and native ARM IPC and signal lifecycle",
            ][..],
        ),
        (
            "carrot-navi-arm",
            "60",
            &[
                "Reserve runner capacity and install native ARM build dependencies",
                "Build and inspect the actual native ARM executable",
            ][..],
        ),
    ] {
        let job = &jobs[name];
        absent(job, &["if", "continue-on-error", "needs"])?;
        require(
            job["env"]["CI296_NATIVE_ARM_CALLER"] == "rust.yml"
                && job["timeout-minutes"] == timeout,
            "native ARM caller scope or timeout changed",
        )?;
        let steps = job["steps"].as_array().ok_or("missing native ARM steps")?;
        for stage in stages {
            let matches = steps
                .iter()
                .filter(|step| step["name"] == *stage)
                .collect::<Vec<_>>();
            require(
                matches.len() == 1,
                format!("missing or duplicate native ARM stage {stage}"),
            )?;
            absent(matches[0], &["continue-on-error"])?;
            let expected = match *stage {
                "Preserve bounded final binaries and their identities" => TRUSTED_ALWAYS,
                "Compare affected original policy, cereal and time boundaries" => {
                    "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (matrix.arch == 'x86_64') }}"
                }
                "Check generated C with ASan, UBSan and allocator leak detection" => {
                    "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (matrix.runner == 'local-x64') }}"
                }
                _ => TRUSTED_SOURCE,
            };
            require(
                matches[0]["if"] == expected,
                format!("native ARM stage condition changed: {stage}"),
            )?;
        }
        if matches!(name, "radar-arm" | "carrot-navi-arm") {
            let label = if name == "radar-arm" {
                "rust RadarCAN native ARM functional validation"
            } else {
                "rust Carrot Navi native ARM build"
            };
            require(
                main["jobs"][name]
                    == serde_json::json!({
                        "name":label, "needs":["native-arm"], "if":"${{ always() }}",
                        "runs-on":HOSTED_RUNNER, "timeout-minutes":"2",
                        "steps":[{"name":"Require native ARM family result",
                            "env":{"SOURCE_ALLOWED":TRUSTED_SOURCE,
                                "NATIVE_ARM":"${{ needs.native-arm.result }}"},
                            "run":format!("{FORK_DENIAL}test \"$NATIVE_ARM\" = success\n")}]
                    }),
                format!("{name}: exact mandatory native ARM proxy changed"),
            )?;
            continue;
        }
        let matrix = &main["jobs"][name]["strategy"]["matrix"];
        require(
            if name == "carrot-man-runtime" {
                matrix == &serde_json::json!({"include":[{"arch":"x86_64","runner":"local-x64"}]})
            } else if name == "radard-runtime" {
                matrix
                    == &serde_json::json!({"include":[{"runner":"local-x64","machine":"x86_64"}]})
            } else {
                matrix == &serde_json::json!({"runner":["local-x64"]})
            },
            "migrated family must retain its exact x64 row",
        )?;
    }
    let gate = &jobs["native-arm-checks"];
    absent(gate, &["continue-on-error", "strategy"])?;
    require(
        strings(&gate["needs"])? == expected_strings(&leaves)
            && gate["needs"]
                .as_array()
                .is_some_and(|v| v.len() == leaves.len())
            && gate["if"] == "${{ always() }}"
            && gate["runs-on"] == HOSTED_RUNNER
            && gate["timeout-minutes"] == "2",
        "native ARM result gate must always require all eight leaves",
    )?;
    let steps = gate["steps"]
        .as_array()
        .ok_or("missing native ARM result step")?;
    require(steps.len() == 1, "native ARM result step roster changed")?;
    absent(&steps[0], &["if", "continue-on-error"])?;
    source_result_binding(&steps[0])?;
    let mut expected_env = serde_json::Map::new();
    expected_env.insert("SOURCE_ALLOWED".into(), TRUSTED_SOURCE.into());
    let mut expected_script = FORK_DENIAL.to_owned();
    for (name, variable) in NATIVE_ARM_BINDINGS {
        expected_env.insert(
            (*variable).into(),
            format!("${{{{ needs.{name}.result }}}}").into(),
        );
        expected_script.push_str(&format!("test \"${variable}\" = success\n"));
    }
    require(
        steps[0]["env"].as_object() == Some(&expected_env) && steps[0]["run"] == expected_script,
        "native ARM result bindings or strict success checks changed",
    )
}

pub fn validate_rust_gate(workflow: &Value) -> Result<(), String> {
    for event in ["push", "pull_request"] {
        require(
            strings(&workflow["on"][event]["branches"])? == expected_strings(&["dev", "main"]),
            format!("Rust {event} branches changed"),
        )?;
        absent(&workflow["on"][event], &["paths", "paths-ignore"])?;
    }
    let jobs = workflow["jobs"].as_object().ok_or("missing Rust jobs")?;
    for job in jobs.values() {
        absent(job, &["continue-on-error"])?;
    }
    require(
        jobs["dbc-darwin"]["uses"] == "./.github/workflows/rust-dbc-darwin.yml",
        "required Darwin job must keep its same-source reusable caller",
    )?;
    absent(&jobs["dbc-darwin"], &["if", "continue-on-error"])?;
    require(
        jobs.get("native-arm")
            == Some(&serde_json::json!({"uses":"./.github/workflows/rust-native-arm.yml"})),
        "required native ARM job must keep its unconditional same-source caller",
    )?;
    let gate = &workflow["jobs"]["fast"];
    require(gate["if"] == "${{ always() }}", "Rust gate must always run")?;
    require(
        strings(&gate["needs"])? == RUST_GATE_BINDINGS.iter().map(|(_, job)| *job).collect(),
        "Rust gate dependency coverage changed",
    )?;
    let steps = gate["steps"].as_array().ok_or("missing Rust gate steps")?;
    let step = steps
        .iter()
        .find(|step| step["env"].get("MEMORY").is_some())
        .ok_or("missing Rust gate validation step")?;
    let bindings = step["env"]
        .as_object()
        .ok_or("missing gate result bindings")?;
    require(
        bindings.len() == RUST_GATE_BINDINGS.len() + 1,
        "Rust gate result binding count changed",
    )?;
    for (name, job) in RUST_GATE_BINDINGS {
        require(
            bindings.get(*name) == Some(&Value::String(format!("${{{{ needs.{job}.result }}}}"))),
            format!("Rust result binding changed: {name}"),
        )?;
    }
    let script = step["run"]
        .as_str()
        .ok_or("missing Rust aggregation script")?;
    source_result_binding(step)?;
    let mut all_success = RUST_GATE_BINDINGS
        .iter()
        .map(|(name, _)| ((*name).into(), "success".into()))
        .collect::<BTreeMap<_, _>>();
    all_success.insert("SOURCE_ALLOWED".into(), "true".into());
    let mut denied = all_success.clone();
    denied.insert("SOURCE_ALLOWED".into(), "false".into());
    require(
        !run_shell(script, &denied)
            .map_err(|e| e.to_string())?
            .success(),
        "Rust fork aggregate must fail",
    )?;
    require(
        run_shell(script, &all_success)
            .map_err(|e| e.to_string())?
            .success(),
        "Rust aggregation rejects successful suites",
    )?;
    for (name, _) in RUST_GATE_BINDINGS {
        for result in ["failure", "cancelled", "skipped", ""] {
            let mut environment = all_success.clone();
            environment.insert((*name).into(), result.into());
            require(
                !run_shell(script, &environment)
                    .map_err(|e| e.to_string())?
                    .success(),
                format!("Rust gate accepts {name}={result:?}"),
            )?;
        }
        let mut environment = all_success.clone();
        environment.remove(*name);
        require(
            !run_shell(script, &environment)
                .map_err(|e| e.to_string())?
                .success(),
            format!("Rust gate accepts absent {name}"),
        )?;
    }
    Ok(())
}
