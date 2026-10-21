use super::{Pin, Reply, Result, ScriptedHttpPeer};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink, path::Path, time::Duration};

pub struct CwpPairConfig<'a> {
    pub repository: &'a Path,
    pub python: &'a Path,
    pub source_script: &'a Path,
    pub source_server: &'a Path,
    pub test_binary: &'a Path,
    pub output: &'a Path,
}

/// Runs the same owned native peer against independently captured source rows.
/// The caller must load and authenticate those rows before calling this API.
pub struct CwpFrozenConfig<'a> {
    pub repository: &'a Path,
    pub source_server: &'a Path,
    pub test_binary: &'a Path,
    pub output: &'a Path,
}

struct RunInputs<'a> {
    repository: &'a Path,
    source_command: Option<(&'a Path, &'a Path)>,
    source_server: &'a Path,
    test_binary: &'a Path,
    output: &'a Path,
}

struct Scenario {
    name: &'static str,
    action: &'static str,
    gate: Option<&'static str>,
    replies: &'static [(u16, &'static str)],
    dongle: &'static str,
    serial: &'static str,
    expected_gate: Option<&'static str>,
}

const fn scenario(
    name: &'static str,
    action: &'static str,
    gate: Option<&'static str>,
    replies: &'static [(u16, &'static str)],
    expected_gate: Option<&'static str>,
) -> Scenario {
    Scenario {
        name,
        action,
        gate,
        replies,
        dongle: "owned-device",
        serial: "owned-hardware",
        expected_gate,
    }
}

// Literal response scripts and gate expectations from the unchanged 14-case
// carrot_recovery_cwp_check.py; no expected value is obtained from Rust.
const CASES: &[Scenario] = &[
    scenario(
        "ready_preserves_enable",
        "status",
        Some("1"),
        &[(200, "{\"ok\":true,\"registered\":true}")],
        Some("1"),
    ),
    scenario(
        "lost_registration_clears_alias",
        "status",
        Some("1"),
        &[(200, "{\"ok\":true,\"registered\":false}")],
        Some("0"),
    ),
    scenario(
        "registered_enable_persists",
        "enable",
        Some("0"),
        &[(200, "{\"ok\":true,\"registered\":true}")],
        Some("1"),
    ),
    scenario(
        "unregistered_enable_rejected",
        "enable",
        Some("1"),
        &[(200, "{\"ok\":true,\"registered\":false}")],
        Some("0"),
    ),
    scenario(
        "unavailable_enable_rejected",
        "enable",
        Some("0"),
        &[(503, "owned service unavailable")],
        Some("0"),
    ),
    scenario(
        "unavailable_disable_persists",
        "disable",
        Some("1"),
        &[(503, "owned service unavailable")],
        Some("0"),
    ),
    scenario(
        "invalid_json_preserves_enable",
        "status",
        Some("1"),
        &[(200, "broken-json")],
        Some("1"),
    ),
    scenario(
        "nonobject_response_preserves_enable",
        "status",
        Some("1"),
        &[(200, "[true]")],
        Some("1"),
    ),
    scenario(
        "application_rejection_preserves_enable",
        "status",
        Some("1"),
        &[(200, "{\"ok\":false,\"registered\":false}")],
        Some("1"),
    ),
    Scenario {
        name: "serial_sentinel_fallback",
        action: "status",
        gate: Some("true"),
        replies: &[(200, "{\"ok\":1,\"registered\":\"yes\"}")],
        dongle: " UNREGISTEREDDEVICE ",
        serial: " owned-hardware \r\n",
        expected_gate: Some("true"),
    },
    Scenario {
        name: "hostname_sentinel_fallback",
        action: "status",
        gate: None,
        replies: &[(200, "{\"ok\":true,\"registered\":true}")],
        dongle: "null",
        serial: "UNKNOWN",
        expected_gate: None,
    },
    scenario("boot_disabled_gate", "boot", Some("0"), &[], Some("0")),
    scenario(
        "boot_registered_preserves_enable",
        "boot",
        Some("1"),
        &[(200, "{\"ok\":true,\"registered\":true,\"pushed\":3}")],
        Some("1"),
    ),
    scenario(
        "boot_retry_then_unregister",
        "boot",
        Some("1"),
        &[
            (503, "transient"),
            (200, "{\"ok\":true,\"registered\":false,\"pushed\":\"2\"}"),
        ],
        Some("0"),
    ),
];

struct Observation {
    result: Value,
    requests: Vec<Value>,
}

fn has_temp_files(directory: &Path) -> Result<bool> {
    for entry in fs::read_dir(directory)? {
        if entry?.file_name().to_string_lossy().starts_with(".tmp_") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_one(
    config: &RunInputs<'_>,
    case: &Scenario,
    provider: &str,
    source_digest: &str,
) -> Result<Observation> {
    let directory = config.output.join(case.name).join(provider);
    fs::create_dir_all(&directory)?;
    let alias = directory.join("params/alias");
    fs::create_dir_all(&alias)?;
    symlink("alias", directory.join("params/d"))?;
    fs::write(alias.join("DongleId"), case.dongle)?;
    fs::write(alias.join("HardwareSerial"), case.serial)?;
    if let Some(gate) = case.gate {
        fs::write(alias.join("CwebPushRecoveryBoot"), gate)?;
    }
    fs::write(directory.join("ip"), "192.168.42.9")?;
    let replies = case
        .replies
        .iter()
        .map(|&(status, body)| Reply::json(status, body.as_bytes()))
        .collect();
    let peer = ScriptedHttpPeer::start(replies)?;
    let argv = if provider == "source" {
        let (python, script) = config.source_command.ok_or("source CWP command absent")?;
        vec![python.display().to_string(), script.display().to_string()]
    } else {
        vec![
            config.test_binary.display().to_string(),
            "--exact".into(),
            "cwp::tests::child_driver".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ]
    };
    let mut command = CommandSpec::new(argv.clone(), Duration::from_secs(20));
    command.cwd = Some(config.repository.to_owned());
    command
        .env
        .insert("CWP_PROBE_ROOT".into(), directory.display().to_string());
    command
        .env
        .insert("CWP_PROBE_ACTION".into(), case.action.into());
    command.env.insert(
        "CWEB_PUSH_REPORT_URL".into(),
        format!("http://127.0.0.1:{}/report", peer.port()),
    );
    command
        .env
        .insert("CWP_REPORT_TOKEN".into(), " owned-loopback-token ".into());
    command.env.insert(
        "PYTHONPYCACHEPREFIX".into(),
        config.output.join("pycache").display().to_string(),
    );
    let mut runner = NativeRunner;
    let captured = runner.run(&command)?;
    fs::write(directory.join("stdout.txt"), &captured.stdout)?;
    fs::write(directory.join("stderr.txt"), &captured.stderr)?;
    let requests = peer.requests()?;
    let rows = requests
        .iter()
        .map(|request| {
            json!({
                "method": request.method,
                "path": request.path,
                "headers": request.headers,
                "body": String::from_utf8_lossy(&request.body),
                "elapsed_seconds": request.elapsed.as_secs_f64(),
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        directory.join("requests.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    fs::write(
        directory.join("invocation.json"),
        serde_json::to_vec_pretty(&json!({"argv":argv,"exit_code":captured.code}))?,
    )?;
    if captured.code != 0 {
        return Err(format!("CWP {}/{provider} exited {}", case.name, captured.code).into());
    }
    let output = std::str::from_utf8(&captured.stdout)?;
    let raw = output
        .lines()
        .find_map(|line| line.strip_prefix("CWP_RESULT="))
        .ok_or("missing CWP_RESULT")?;
    let result: Value = serde_json::from_str(raw)?;
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    if provider == "source" {
        let proof: Value = serde_json::from_slice(&fs::read(directory.join("source-proof.json"))?)?;
        if proof.get("source").and_then(Value::as_str)
            != config.source_server.canonicalize()?.to_str()
            || proof.get("sha256").and_then(Value::as_str) != Some(source_digest)
            || proof.get("runtime_functions_changed") != Some(&Value::Bool(false))
        {
            return Err("unchanged CWP server source proof mismatch".into());
        }
    }
    let gate_path = alias.join("CwebPushRecoveryBoot");
    let observed_gate = if gate_path.is_file() {
        Some(fs::read_to_string(&gate_path)?)
    } else {
        None
    };
    if observed_gate.as_deref() != case.expected_gate
        || requests.len() != case.replies.len()
        || !directory.join("params/d").is_symlink()
        || has_temp_files(&alias)?
        || peer.pending_replies()? != 0
    {
        return Err(format!(
            "CWP {}/{provider} gate or request contract failed",
            case.name
        )
        .into());
    }
    let mut payloads = Vec::new();
    for request in &requests {
        let header = |key: &str| {
            request
                .headers
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        if request.method != "POST"
            || request.path
                != format!(
                    "/recovery/{}",
                    if case.action == "boot" {
                        "boot"
                    } else {
                        "status"
                    }
                )
            || header("accept") != Some("application/json")
            || header("content-type") != Some("application/json")
            || header("user-agent") != Some("CarrotRecovery/2.0")
            || header("authorization") != Some("Bearer owned-loopback-token")
        {
            return Err(format!("CWP {}/{provider} request header mismatch", case.name).into());
        }
        payloads.push(serde_json::from_slice(&request.body)?);
    }
    if case.name == "hostname_sentinel_fallback" {
        let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?;
        if payloads
            .first()
            .and_then(|row: &Value| row.get("deviceId"))
            .and_then(Value::as_str)
            != Some(hostname.trim_end_matches(['\n', '\r']))
        {
            return Err("CWP kernel hostname fallback mismatch".into());
        }
    }
    if case.action == "boot"
        && !requests.is_empty()
        && (requests[0].elapsed < Duration::from_secs(2)
            || (requests.len() > 1
                && requests[1].elapsed.saturating_sub(requests[0].elapsed)
                    < Duration::from_secs(5)))
    {
        return Err(format!("CWP {}/{provider} boot cadence mismatch", case.name).into());
    }
    Ok(Observation {
        result,
        requests: payloads,
    })
}

pub fn run_cwp_pair(config: &CwpPairConfig<'_>) -> Result<()> {
    fs::create_dir(config.output)?;
    let source = Pin::open(config.source_script, 1024 * 1024)?;
    let source_server = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let binary = Pin::open(config.test_binary, 256 * 1024 * 1024)?;
    let inputs = RunInputs {
        repository: config.repository,
        source_command: Some((config.python, config.source_script)),
        source_server: config.source_server,
        test_binary: config.test_binary,
        output: config.output,
    };
    let mut rows = Vec::new();
    for case in CASES {
        let original = run_one(&inputs, case, "source", source_server.digest())?;
        let native = run_one(&inputs, case, "native", source_server.digest())?;
        let row = json!({"scenario":case.name,"original":original.result,"native":native.result,"source_requests":original.requests,"native_requests":native.requests});
        rows.push(row);
        fs::write(
            config.output.join("comparisons.json"),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        let pair = rows.last().ok_or("missing CWP comparison")?;
        if pair["original"] != pair["native"] || pair["source_requests"] != pair["native_requests"]
        {
            return Err(format!("CWP source/native mismatch: {}", case.name).into());
        }
    }
    source.verify()?;
    source_server.verify()?;
    binary.verify()?;
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"pass":true,"cases":rows.len(),"source_sha256":source.digest(),"source_server_sha256":source_server.digest(),"binary_sha256":binary.digest()}),
        )?,
    )?;
    Ok(())
}

fn frozen_payloads(case: &Scenario, source: &Value) -> Result<Vec<Value>> {
    let rows = source
        .as_array()
        .ok_or("frozen CWP requests are not an array")?;
    if rows.len() != case.replies.len() {
        return Err("frozen CWP request count changed".into());
    }
    let mut payloads = Vec::with_capacity(rows.len());
    for row in rows {
        let object = row
            .as_object()
            .ok_or("frozen CWP request is not an object")?;
        if object.len() != 8
            || row["method"] != "POST"
            || row["path"]
                != format!(
                    "/recovery/{}",
                    if case.action == "boot" {
                        "boot"
                    } else {
                        "status"
                    }
                )
            || row["accept"] != "application/json"
            || row["content_type"] != "application/json"
            || row["user_agent"] != "CarrotRecovery/2.0"
            || row["authorization"] != "Bearer owned-loopback-token"
        {
            return Err("frozen CWP request envelope changed".into());
        }
        let elapsed = row["elapsed"]
            .as_str()
            .ok_or("frozen CWP elapsed time absent")?
            .parse::<f64>()?;
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err("frozen CWP elapsed time invalid".into());
        }
        let payload: Value =
            serde_json::from_str(row["payload"].as_str().ok_or("frozen CWP payload absent")?)?;
        let mut payload = payload
            .as_object()
            .ok_or("frozen CWP payload is not an object")?
            .clone();
        if case.name == "hostname_sentinel_fallback" {
            let captured = payload
                .get("deviceId")
                .and_then(Value::as_str)
                .ok_or("frozen CWP hostname absent")?;
            if captured.is_empty() {
                return Err("frozen CWP hostname empty".into());
            }
            let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?;
            payload.insert(
                "deviceId".into(),
                Value::String(hostname.trim_end_matches(['\n', '\r']).into()),
            );
        }
        payloads.push(Value::Object(payload));
    }
    Ok(payloads)
}

/// Runs all 14 owned CWP cases against authenticated original-only observations.
/// `load_original` must resolve names from a separately pinned, source-only
/// fixture; the observations cannot be derived from this native invocation.
pub fn run_cwp_frozen(
    config: &CwpFrozenConfig<'_>,
    load_original: impl Fn(&str) -> Result<Value>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let source_server = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let binary = Pin::open(config.test_binary, 256 * 1024 * 1024)?;
    let inputs = RunInputs {
        repository: config.repository,
        source_command: None,
        source_server: config.source_server,
        test_binary: config.test_binary,
        output: config.output,
    };
    let mut rows = Vec::with_capacity(CASES.len());
    for case in CASES {
        let proof = load_original(&format!("cwp/{}/source/source-proof.json", case.name))?;
        if proof["sha256"].as_str() != Some(source_server.digest())
            || proof["runtime_functions_changed"] != false
            || !proof["source"].as_str().is_some_and(|path| {
                path.ends_with("/openpilot/selfdrive/carrot/recovery/server.py")
            })
            || proof["executed_functions"]
                .as_array()
                .is_none_or(|functions| functions.len() != 8)
        {
            return Err(format!("frozen CWP source proof changed: {}", case.name).into());
        }
        let invocation = load_original(&format!("cwp/{}/source/invocation.json", case.name))?;
        let command = invocation["command"]
            .as_array()
            .ok_or("frozen CWP invocation absent")?;
        if invocation["exit_code"] != 0
            || command.len() != 2
            || !command[1]
                .as_str()
                .is_some_and(|path| path.ends_with("/rust/tools/carrot_recovery_cwp_source.py"))
        {
            return Err(format!("frozen CWP invocation changed: {}", case.name).into());
        }
        let result = load_original(&format!("cwp/{}/source/result.json", case.name))?;
        let requests = load_original(&format!("cwp/{}/source/requests.json", case.name))?;
        let source_payloads = frozen_payloads(case, &requests)?;
        let native = run_one(&inputs, case, "native", source_server.digest())?;
        let row = json!({"scenario":case.name,"original":result,"native":native.result,
            "source_requests":source_payloads,"native_requests":native.requests});
        rows.push(row);
        fs::write(
            config.output.join("comparisons.json"),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        let pair = rows.last().ok_or("missing frozen CWP comparison")?;
        if pair["original"] != pair["native"] || pair["source_requests"] != pair["native_requests"]
        {
            return Err(format!("frozen CWP/native mismatch: {}", case.name).into());
        }
    }
    source_server.verify()?;
    binary.verify()?;
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"cases":rows.len(),
            "original_source_only":true,"source_server_sha256":source_server.digest(),
            "binary_sha256":binary.digest()}))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod frozen_tests {
    use super::*;

    #[test]
    fn source_request_projection_rejects_changed_envelope_and_missing_replies() {
        let row = json!({"method":"POST","path":"/recovery/status",
            "payload":"{\"deviceId\": \"owned-device\"}","accept":"application/json",
            "content_type":"application/json","user_agent":"CarrotRecovery/2.0",
            "authorization":"Bearer owned-loopback-token","elapsed":"0.125"});
        assert_eq!(
            frozen_payloads(&CASES[0], &json!([row.clone()])).unwrap(),
            vec![json!({"deviceId":"owned-device"})]
        );
        let mut wrong_path = row.clone();
        wrong_path["path"] = json!("/recovery/other");
        assert!(frozen_payloads(&CASES[0], &json!([wrong_path])).is_err());
        let mut wrong_header = row.clone();
        wrong_header["authorization"] = json!("Bearer replacement");
        assert!(frozen_payloads(&CASES[0], &json!([wrong_header])).is_err());
        let mut wrong_time = row;
        wrong_time["elapsed"] = json!("NaN");
        assert!(frozen_payloads(&CASES[0], &json!([wrong_time])).is_err());
        assert!(frozen_payloads(&CASES[0], &json!([])).is_err());
    }
}
