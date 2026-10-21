//! Retains the unchanged upload controller as an independent source oracle.
//! Its HTTP upload and multipart comparisons remain mandatory during this
//! staged controller conversion; this is not a Python absence proof.

use super::{upload_wire, HttpPeer, OwnedServer, Pin, Result, ScriptedHttpPeer};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use rustix::process::Signal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString, fs, net::TcpListener, os::unix::fs::PermissionsExt, path::Path, time::Duration,
};

pub struct UploadCorpusConfig<'a> {
    pub repository: &'a Path,
    pub python: &'a Path,
    pub controller: &'a Path,
    pub fixtures: &'a Path,
    pub source_server: &'a Path,
    pub candidate: &'a Path,
    pub actions_binary: &'a Path,
    pub process_child: &'a Path,
    pub output: &'a Path,
}

fn run_http_action(
    config: &UploadCorpusConfig<'_>,
    row: &Value,
    name: &str,
    output: &Path,
) -> Result<Value> {
    fs::create_dir(output)?;
    let expected_requests = row["source_requests"]
        .as_array()
        .ok_or("missing original HTTP action requests")?;
    let scripted = expected_requests
        .iter()
        .map(|request| {
            let method = if request["body"].is_null() {
                "GET"
            } else {
                "POST"
            };
            let path = request["path"]
                .as_str()
                .ok_or("missing original HTTP action path")?;
            Ok(upload_wire::scripted_response(name, method, path))
        })
        .collect::<Result<Vec<_>>>()?;
    let recipient = ScriptedHttpPeer::start(scripted)?;
    let tools = output.join("tools");
    fs::create_dir(&tools)?;
    let tmux = tools.join("tmux");
    fs::write(&tmux, b"#!/bin/sh\nprintf 'owned capture\\377\\r\\n'\n")?;
    fs::set_permissions(&tmux, fs::Permissions::from_mode(0o755))?;
    let path = std::env::var_os("PATH").ok_or("PATH required for recovery tool")?;
    let mut owned_path = tools.as_os_str().to_os_string();
    owned_path.push(":");
    owned_path.push(path);
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        OsString::from(config.actions_binary),
        OsString::from(config.output),
        OsString::from(output.join("tmux.log")),
        OsString::from(config.process_child),
        OsString::from(port.to_string()),
    ];
    let base = format!("http://127.0.0.1:{}", recipient.port());
    let environment = [
        (OsString::from("PATH"), owned_path),
        (
            OsString::from("CARROT_WEB_UPLOAD_URL"),
            OsString::from(&base),
        ),
        (
            OsString::from("CARROT_TMUX_WEB_UPLOAD_URL"),
            OsString::from(format!("{base}/logs")),
        ),
        (
            OsString::from("CARROT_EXCEPTION_DISCORD_WEBHOOK_URL"),
            OsString::from(format!("{base}/discord")),
        ),
        (
            OsString::from("CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE"),
            OsString::from("0"),
        ),
        (
            OsString::from("CARROT_WEB_UPLOAD_TOKEN"),
            OsString::from(""),
        ),
        (
            OsString::from("CARROT_DATA_DIR"),
            OsString::from(config.output.join("data")),
        ),
    ];
    let mut native = OwnedServer::spawn(
        &argv,
        config.repository,
        &output.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(5));
    let observed = (|| -> Result<(u16, Value)> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(10))?;
        let response = peer.request("POST", "/api/action", br#"{"action":"server_tmux_log"}"#)?;
        Ok((response.status, serde_json::from_slice(&response.body)?))
    })();
    let stop = native.signal(Signal::TERM);
    let exit = native.finish(Duration::from_secs(5));
    let (status, actual) = observed?;
    stop?;
    let exit = exit?;
    let requests = recipient
        .requests()?
        .iter()
        .map(upload_wire::normalized)
        .collect::<Result<Vec<_>>>()?;
    let capture = fs::read(output.join("tmux.log"))?;
    let pass = status == 200
        && actual == row["source"]
        && requests.as_slice() == expected_requests.as_slice()
        && recipient.pending_replies()? == 0
        && capture == b"owned capture\xef\xbf\xbd\n";
    let receipt = json!({"scenario":name,"source":row["source"],"native":actual,
        "source_requests":expected_requests,"native_requests":requests,"http_status":status,
        "exit":exit,"capture_sha256":format!("{:x}",Sha256::digest(&capture)),"capture_len":capture.len(),"pass":pass});
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !pass {
        return Err(format!("Rust-driven recovery HTTP action mismatch: {name}").into());
    }
    Ok(receipt)
}

const MODES: &[&str] = &[
    "success",
    "token",
    "session-denied",
    "missing-token",
    "non-json",
    "partial",
    "all-failed",
    "empty-error",
    "disabled",
    "invalid-urls",
    "large",
    "missing-file",
    "redirect301",
    "redirect307",
    "large-response",
    "invalid-utf8-response",
];

pub fn admit_upload_corpus(config: &UploadCorpusConfig<'_>) -> Result<()> {
    if config.output.exists() {
        return Err("recovery upload output must be fresh".into());
    }
    let pins = [
        Pin::open(config.controller, 1024 * 1024)?,
        Pin::open(config.fixtures, 1024 * 1024)?,
        Pin::open(config.source_server, 4 * 1024 * 1024)?,
        Pin::open(config.candidate, 256 * 1024 * 1024)?,
        Pin::open(config.actions_binary, 256 * 1024 * 1024)?,
        Pin::open(config.process_child, 256 * 1024 * 1024)?,
    ];
    let argv = vec![
        config.python.display().to_string(),
        "-P".into(),
        config.controller.display().to_string(),
        "--binary".into(),
        config.candidate.display().to_string(),
        "--output".into(),
        config.output.display().to_string(),
        "--http-binary".into(),
        config.actions_binary.display().to_string(),
    ];
    let mut command = CommandSpec::new(argv.clone(), Duration::from_secs(12 * 60));
    command.cwd = Some(config.repository.to_owned());
    command.env_remove.push("PYTHONOPTIMIZE".into());
    command.stdout_limit = 1024 * 1024;
    command.stderr_limit = 4 * 1024 * 1024;
    let mut runner = NativeRunner;
    let observed = runner.run(&command)?;
    if !config.output.exists() {
        return Err("recovery upload output missing".into());
    }
    fs::write(config.output.join("controller.stdout"), &observed.stdout)?;
    fs::write(config.output.join("controller.stderr"), &observed.stderr)?;
    fs::write(
        config.output.join("invocation.json"),
        serde_json::to_vec_pretty(&json!({"argv":argv,"exit":observed.code}))?,
    )?;
    if observed.code != 0 {
        return Err("unchanged recovery upload corpus failed".into());
    }
    let summary: Value = serde_json::from_slice(&fs::read(config.output.join("result.json"))?)?;
    if summary["pairs"].as_u64() != Some(MODES.len() as u64)
        || summary["http_action_pairs"].as_u64() != Some(3)
        || summary["source_sha256"].as_str() != Some(pins[2].digest())
        || summary["binary_sha256"].as_str() != Some(pins[3].digest())
        || summary["owned_recipients_only"] != true
    {
        return Err("recovery upload summary changed".into());
    }
    let rows: Vec<Value> =
        serde_json::from_slice(&fs::read(config.output.join("comparisons.json"))?)?;
    if rows.len() != MODES.len() {
        return Err("recovery upload scenario count changed".into());
    }
    for (name, row) in MODES.iter().zip(&rows) {
        if row["scenario"] != *name
            || !row["source"].is_object()
            || !row["native"].is_object()
            || !row["source_requests"].is_array()
            || !row["native_requests"].is_array()
            || row.get("source") != row.get("native")
            || row.get("source_requests") != row.get("native_requests")
        {
            return Err(format!("recovery upload source/native mismatch: {name}").into());
        }
    }
    let http: Vec<Value> =
        serde_json::from_slice(&fs::read(config.output.join("http/comparisons.json"))?)?;
    if http.len() != 3 {
        return Err("recovery upload HTTP action count changed".into());
    }
    for (name, row) in ["success", "partial", "all-failed"].iter().zip(&http) {
        if row["scenario"] != *name
            || !row["source"].is_object()
            || !row["native"].is_object()
            || !row["source_requests"].is_array()
            || !row["native_requests"].is_array()
            || row.get("source") != row.get("native")
            || row.get("source_requests") != row.get("native_requests")
        {
            return Err(format!("recovery upload HTTP action mismatch: {name}").into());
        }
    }
    let direct_actions = config.output.join("direct-http-actions");
    fs::create_dir(&direct_actions)?;
    let mut action_rows = Vec::new();
    for (name, row) in ["success", "partial", "all-failed"].iter().zip(&http) {
        action_rows.push(run_http_action(
            config,
            row,
            name,
            &direct_actions.join(name),
        )?);
        fs::write(
            direct_actions.join("comparisons.json"),
            serde_json::to_vec_pretty(&action_rows)?,
        )?;
    }
    let mut direct_rows = Vec::new();
    let direct_output = config.output.join("direct-native");
    fs::create_dir(&direct_output)?;
    for (name, row) in MODES.iter().zip(&rows) {
        let expected_requests = row["source_requests"]
            .as_array()
            .ok_or("missing original upload requests")?;
        let scripted = expected_requests
            .iter()
            .map(|request| {
                let method = if request["body"].is_null() {
                    "GET"
                } else {
                    "POST"
                };
                let path = request["path"]
                    .as_str()
                    .ok_or("missing original upload request path")?;
                Ok(upload_wire::scripted_response(name, method, path))
            })
            .collect::<Result<Vec<_>>>()?;
        let recipient = ScriptedHttpPeer::start(scripted)?;
        let tmux = config.output.join("tmux.log");
        if *name == "missing-file" {
            if tmux.exists() {
                fs::remove_file(&tmux)?;
            }
        } else if *name == "large" {
            fs::write(
                &tmux,
                (0_u8..=255).cycle().take(256 * 33000).collect::<Vec<_>>(),
            )?;
        } else {
            fs::write(&tmux, b"owned-log\x00\xff\n")?;
        }
        let base = format!("http://127.0.0.1:{}", recipient.port());
        let url = if *name == "invalid-urls" {
            "invalid".to_owned()
        } else {
            base.clone()
        };
        let mut replay = CommandSpec::new(
            vec![
                config.candidate.display().to_string(),
                config.output.display().to_string(),
                "owned-recovery".into(),
            ],
            Duration::from_secs(45),
        );
        replay.cwd = Some(config.repository.to_owned());
        replay
            .env
            .insert("CARROT_WEB_UPLOAD_URL".into(), url.clone());
        replay.env.insert(
            "CARROT_TMUX_WEB_UPLOAD_URL".into(),
            if *name == "invalid-urls" {
                url.clone()
            } else {
                format!("{base}/logs")
            },
        );
        replay.env.insert(
            "CARROT_EXCEPTION_DISCORD_WEBHOOK_URL".into(),
            if *name == "invalid-urls" {
                url
            } else {
                format!("{base}/discord")
            },
        );
        replay.env.insert(
            "CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE".into(),
            if *name == "disabled" { "1" } else { "0" }.into(),
        );
        replay.env.insert(
            "CARROT_WEB_UPLOAD_TOKEN".into(),
            if *name == "token" { "owned-token" } else { "" }.into(),
        );
        replay.env.insert(
            "CARROT_DATA_DIR".into(),
            config.output.join("data").display().to_string(),
        );
        let rerun = runner.run(&replay)?;
        fs::write(direct_output.join(format!("{name}.stdout")), &rerun.stdout)?;
        fs::write(direct_output.join(format!("{name}.stderr")), &rerun.stderr)?;
        let observed_requests = recipient
            .requests()?
            .iter()
            .map(upload_wire::normalized)
            .collect::<Result<Vec<_>>>()?;
        let actual: Value = serde_json::from_slice(&rerun.stdout)?;
        let matching = rerun.code == 0
            && actual == row["source"]
            && observed_requests.as_slice() == expected_requests.as_slice()
            && recipient.pending_replies()? == 0;
        direct_rows.push(json!({"scenario":name,"source":row["source"],"native":actual,"source_requests":expected_requests,"native_requests":observed_requests,"exit":rerun.code}));
        fs::write(
            config.output.join("direct-native.json"),
            serde_json::to_vec_pretty(&direct_rows)?,
        )?;
        if !matching {
            return Err(format!("Rust-driven recovery upload mismatch: {name}").into());
        }
    }
    for pin in &pins {
        pin.verify()?;
    }
    fs::write(
        config.output.join("admission.json"),
        serde_json::to_vec_pretty(&json!({
            "pass":true,"source_controller_required":true,"upload_modes":MODES.len(),
        "http_action_modes":3,"rust_driven_native_modes":direct_rows.len(),"rust_driven_http_actions":action_rows.len(),"source_sha256":pins[2].digest(),
            "candidate_sha256":pins[3].digest(),"actions_sha256":pins[4].digest(),
        }))?,
    )?;
    Ok(())
}
