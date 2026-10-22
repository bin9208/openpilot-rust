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

pub fn check_workflow_policy(files: &[(String, String)]) -> Result<(), String> {
    let workflows = files
        .iter()
        .map(|(name, text)| parse(text).map(|v| (name.as_str(), v)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let get = |name: &str| {
        workflows
            .get(name)
            .ok_or_else(|| format!("missing workflow {name}"))
    };
    let fast = get("fast-checks.yml")?;
    require(
        fast["on"]["push"]["branches"] == serde_json::json!(["**"]),
        "fast checks must cover every branch push",
    )?;
    require(
        strings(&fast["on"]["pull_request"]["branches"])? == expected_strings(&["dev", "main"]),
        "fast protected PR branches changed",
    )?;
    for event in ["push", "pull_request"] {
        absent(&fast["on"][event], &["paths", "paths-ignore"])?;
    }
    validate_fast_gate(fast)?;
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
        bindings.len() == RUST_GATE_BINDINGS.len(),
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
    let all_success = RUST_GATE_BINDINGS
        .iter()
        .map(|(name, _)| ((*name).into(), "success".into()))
        .collect::<BTreeMap<_, _>>();
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
