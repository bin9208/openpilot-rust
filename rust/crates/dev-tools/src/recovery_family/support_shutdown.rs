use super::{HttpPeer, OwnedServer, Pin, Result};
use rustix::process::Signal;
use serde_json::json;
use std::{
    ffi::OsString,
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub struct SupportShutdownConfig<'a> {
    pub repository: &'a Path,
    pub candidate: &'a Path,
    pub output: &'a Path,
}

fn state(pid: u32) -> Result<Option<String>> {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(raw) => Ok(Some(
            raw.rsplit_once(')')
                .ok_or("invalid owned tunnel stat")?
                .1
                .trim_start()
                .chars()
                .next()
                .ok_or("missing owned tunnel state")?
                .to_string(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn run_support_shutdown(config: &SupportShutdownConfig<'_>) -> Result<()> {
    run_support_shutdown_inner(config, None)
}

/// Requires the same pending-tunnel termination and request-disconnect
/// relations observed by the unchanged original controller. The source rows
/// must come from the independently authenticated original-only fixture.
pub fn run_support_shutdown_frozen(
    config: &SupportShutdownConfig<'_>,
    load_original: impl Fn(&str) -> Result<serde_json::Value>,
) -> Result<()> {
    run_support_shutdown_inner(config, Some(&load_original))
}

fn run_support_shutdown_inner(
    config: &SupportShutdownConfig<'_>,
    load_original: Option<&dyn Fn(&str) -> Result<serde_json::Value>>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let tunnel = config.output.join("cloudflared");
    let shell = config.output.join("fixture-shell");
    fs::write(&tunnel, "#!/bin/sh\nprintf \"%s\\n\" \"$$\" > \"$CARROT_OWNED_TUNNEL_ENTERED\"\nwhile ! test -f \"$CARROT_OWNED_TUNNEL_RELEASE\"; do sleep 0.1; done\nprintf \"https://owned-fixture.trycloudflare.com\\n\"\nexec sleep 600\n")?;
    fs::write(&shell, "#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n")?;
    fs::set_permissions(&tunnel, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700))?;
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        OsString::from(config.candidate),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string().into(),
    ];
    let entered = config.output.join("entered");
    let release = config.output.join("release");
    let env = [
        (OsString::from("USER"), OsString::from("comma")),
        (OsString::from("SHELL"), OsString::from(&shell)),
        (
            OsString::from("CARROT_CLOUDFLARED_BIN"),
            OsString::from(&tunnel),
        ),
        (
            OsString::from("CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE"),
            OsString::from("1"),
        ),
        (
            OsString::from("CARROT_OWNED_TUNNEL_ENTERED"),
            OsString::from(&entered),
        ),
        (
            OsString::from("CARROT_OWNED_TUNNEL_RELEASE"),
            OsString::from(&release),
        ),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        config.repository,
        &config.output.join("process.log"),
        &env,
        &["CARROT_SUPPORT_FAKE_TUNNEL_URL"],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(8));
    let result = (|| -> Result<()> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        let start = thread::spawn(move || {
            peer.request(
                "POST",
                "/api/support_terminal/start",
                br#"{"note":"owned cancellation"}"#,
            )
            .map(|response| response.status)
            .map_err(|error| error.to_string())
        });
        let entered_deadline = Instant::now() + Duration::from_secs(5);
        while !entered.exists() {
            if Instant::now() >= entered_deadline {
                fs::write(&release, "release on fixture cleanup\n")?;
                return Err("owned recovery tunnel did not enter".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        let tunnel_pid: u32 = fs::read_to_string(&entered)?.trim().parse()?;
        let began = Instant::now();
        server.signal(Signal::TERM)?;
        let first_wait = server.wait_unreaped(Duration::from_secs(3));
        let exited = match &first_wait {
            Ok(_) => true,
            Err(error) if error.to_string() == "recovery server shutdown timeout" => false,
            Err(error) => return Err(error.to_string().into()),
        };
        fs::write(
            config.output.join("observation.json"),
            serde_json::to_vec_pretty(
                &json!({"scenario":"SIGTERM during real pending support tunnel startup","tunnel_entered":true,"tunnel_pid":tunnel_pid,"exited_within_three_seconds":exited,"seconds_before_release":began.elapsed().as_secs_f64(),"expect_blocked":false}),
            )?,
        )?;
        if !exited {
            fs::write(&release, "release owned URL provider\n")?;
        }
        let observed_exit = if exited {
            first_wait?
        } else {
            server.wait_unreaped(Duration::from_secs(8))?
        };
        // Observe the tunnel before the QA owner's eventual process-group
        // cleanup. Otherwise this test would hide a candidate orphan.
        let tunnel_state = state(tunnel_pid)?;
        let request = start
            .join()
            .map_err(|_| "recovery pending request thread panicked")?;
        fs::write(
            config.output.join("request.json"),
            serde_json::to_vec_pretty(
                &json!({"completed":request.is_ok(),"status":request.as_ref().ok()}),
            )?,
        )?;
        fs::write(
            config.output.join("cleanup.json"),
            serde_json::to_vec_pretty(
                &json!({"observed_exit":observed_exit,"tunnel_state_before_owner_cleanup":tunnel_state}),
            )?,
        )?;
        let owner_exit = server.finish(Duration::from_secs(1))?;
        if observed_exit != 0 || owner_exit != observed_exit || tunnel_state.is_some() || !exited {
            return Err("recovery pending support shutdown failed".into());
        }
        if let Some(load) = load_original {
            let source_observation = load("shutdown/source/observation.json")?;
            let source_request = load("shutdown/source/request.json")?;
            let source_cleanup = load("shutdown/source/cleanup.json")?;
            let source_result = load("shutdown/source/result.json")?;
            let native_observation: serde_json::Value =
                serde_json::from_slice(&fs::read(config.output.join("observation.json"))?)?;
            let native_request: serde_json::Value =
                serde_json::from_slice(&fs::read(config.output.join("request.json"))?)?;
            let native_cleanup: serde_json::Value =
                serde_json::from_slice(&fs::read(config.output.join("cleanup.json"))?)?;
            let mut source_result_observation = source_result.clone();
            source_result_observation
                .as_object_mut()
                .ok_or("original shutdown result not an object")?
                .remove("expected_observable_confirmed");
            if source_result_observation != source_observation {
                return Err("original shutdown observation/result changed".into());
            }
            let expected_scenario = "SIGTERM during real pending support tunnel startup";
            for row in [&source_observation, &source_result, &native_observation] {
                if row["scenario"] != expected_scenario
                    || row["tunnel_entered"] != true
                    || row["exited_within_three_seconds"] != true
                    || row["expect_blocked"] != false
                    || !row["tunnel_pid"].as_u64().is_some_and(|pid| pid > 0)
                    || !row["seconds_before_release"]
                        .as_f64()
                        .is_some_and(|elapsed| {
                            elapsed.is_finite() && (0.0..=3.0).contains(&elapsed)
                        })
                {
                    return Err("original/native pending tunnel shutdown relation changed".into());
                }
            }
            if source_result["expected_observable_confirmed"] != true
                || source_request["closed"] != "ServerDisconnectedError"
                || source_cleanup["exit"] != 0
                || source_cleanup.get("tunnel_state_after") != Some(&serde_json::Value::Null)
                || native_request["completed"] != false
                || native_request.get("status") != Some(&serde_json::Value::Null)
                || native_cleanup["observed_exit"] != 0
                || native_cleanup.get("tunnel_state_before_owner_cleanup")
                    != Some(&serde_json::Value::Null)
            {
                return Err("original/native pending tunnel request or cleanup changed".into());
            }
        }
        candidate.verify()?;
        fs::write(
            config.output.join("result.json"),
            serde_json::to_vec_pretty(
                &json!({"pass":true,"exited_within_three_seconds":true,"candidate_sha256":candidate.digest(),"frozen_original_compared":load_original.is_some()}),
            )?,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::write(&release, "release on owned failure cleanup\n");
        let _ = server.signal(Signal::TERM);
    }
    result
}
