//! Native CI routing checks. YAML scalars deliberately remain strings, matching
//! the former PyYAML BaseLoader rather than YAML's implicit type resolution.
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{Map, Value};
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
pub const TRUSTED_SOURCE: &str = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}";
pub const TRUSTED_SUCCESS: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (success()) }}";
pub const TRUSTED_ALWAYS: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always()) }}";
pub const FORK_DENIAL: &str = "test \"${SOURCE_ALLOWED:-}\" = true || { echo 'Fork PR source denied on persistent bin-actions' >&2; exit 1; }\n";
fn source_result_binding(step: &Value) -> Result<(), String> {
    absent(step, &["if", "continue-on-error"])?;
    require(
        step["env"]["SOURCE_ALLOWED"] == TRUSTED_SOURCE,
        "missing exact source trust binding",
    )?;
    require(
        step["run"]
            .as_str()
            .is_some_and(|s| s.starts_with(FORK_DENIAL)),
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
                        | "Require unavailable Darwin execution interface"
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
        let finish = position("Finish emulated architecture evidence")?;
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
    expected.extend([
        "steps.ci296_source_seal.outcome == 'success'".into(),
        "env.CI296_SOURCE_OWNER != ''".into(),
    ]);
    require(
        condition == private_source_condition(&expected.join(" && ")),
        "source cleanup requires the exact successful seal and upload receipts",
    )?;
    let common = &steps[position("Clean common owned temporary paths after uploaded source")?];
    let mut expected = private_upload_conditions(steps)?;
    expected.splice(1..1, native_success.iter().cloned());
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

/// Every current or newly published diagnostic workflow is checked, not just
/// the inherited Fast/Rust files. Matrix roles never select hosted machines.
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
        require(
            job["runs-on"] == serde_json::json!(LOCAL_RUNNER),
            format!("{name}: fixed bin-actions labels required; no hosted/matrix fallback"),
        )?;
        validate_private_job_layout(job)?;
        let steps = job["steps"]
            .as_array()
            .ok_or("missing source trust boundary")?;
        source_result_binding(steps.first().ok_or("missing fork refusal step")?)?;
        if steps[0]["name"] == "Deny fork source and require registered local runner" {
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
                job["if"] == "${{ always() }}",
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
pub const DIAGNOSTIC_CLEANUP: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.fast_evidence.outcome == 'success' && steps.fast_evidence.outputs.artifact-id != '' && steps.fast_evidence.outputs.artifact-digest != '' && env.DIAG_SOURCE_OWNER != '') }}";
pub const DIAGNOSTIC_COMMON_CLEANUP: &str = "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.fast_evidence.outcome == 'success' && steps.fast_evidence.outputs.artifact-id != '' && steps.fast_evidence.outputs.artifact-digest != '' && steps.fast_source_cleanup.outcome == 'success') }}";

/// Preserve the required fast-check status while its six mandatory workers run
/// independently. Execute the aggregate shell to reject accidental fail-open
/// edits, including absent results that GitHub exposes as empty strings.
pub fn validate_fast_gate(workflow: &Value) -> Result<(), String> {
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
            worker["runs-on"] == serde_json::json!(DIAGNOSTIC_LOCAL_RUNNER),
            "Fast workers require fixed bin-actions labels",
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
        for step in &steps[1..] {
            absent(step, &["continue-on-error"])?;
            let condition = match step["name"].as_str().unwrap_or("") {
                "Retain Fast job ownership evidence" => DIAGNOSTIC_PROOF,
                "Clean only uploaded private Fast source" => DIAGNOSTIC_CLEANUP,
                "Clean common owned Fast job temporary paths" => DIAGNOSTIC_COMMON_CLEANUP,
                _ => DIAGNOSTIC_TRUSTED_SOURCE,
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
        gate["runs-on"] == serde_json::json!(DIAGNOSTIC_LOCAL_RUNNER),
        "Fast aggregate requires bin-actions",
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
];
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
