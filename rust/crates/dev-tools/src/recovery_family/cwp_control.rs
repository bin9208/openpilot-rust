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
    config: &CwpPairConfig<'_>,
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
        vec![
            config.python.display().to_string(),
            config.source_script.display().to_string(),
        ]
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
    if case.action == "boot" && !requests.is_empty() {
        if requests[0].elapsed < Duration::from_secs(2)
            || (requests.len() > 1
                && requests[1].elapsed.saturating_sub(requests[0].elapsed) < Duration::from_secs(5))
        {
            return Err(format!("CWP {}/{provider} boot cadence mismatch", case.name).into());
        }
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
    let mut rows = Vec::new();
    for case in CASES {
        let original = run_one(config, case, "source", source_server.digest())?;
        let native = run_one(config, case, "native", source_server.digest())?;
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
