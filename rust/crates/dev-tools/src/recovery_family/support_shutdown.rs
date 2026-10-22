use super::{HttpPeer, OwnedServer, Pin, Result};
use crate::recovery_exec_audit::CreatedProgramGuard;
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

type OriginalObservation<'a> = dyn Fn(&str) -> Result<serde_json::Value> + 'a;

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

fn wait_for_published_pid(path: &Path, timeout: Duration) -> Result<u32> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs::read_to_string(path) {
            Ok(raw) => {
                // Shell redirection creates the file before printf publishes
                // its PID. Existence, or a partial decimal prefix, is not ready.
                if let Some(line) = raw.strip_suffix('\n') {
                    if line.is_empty() || !line.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err("invalid owned tunnel PID publication".into());
                    }
                    let pid: u32 = line.parse()?;
                    if pid == 0 {
                        return Err("invalid owned tunnel PID publication".into());
                    }
                    return Ok(pid);
                }
                if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err("invalid owned tunnel PID publication".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err("owned recovery tunnel did not publish a complete PID".into());
        };
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

/// Requires the same pending-tunnel termination and request-disconnect
/// relations observed by the unchanged original controller. The source rows
/// must come from the independently authenticated original-only fixture.
pub fn run_support_shutdown_frozen(
    config: &SupportShutdownConfig<'_>,
    load_original: impl Fn(&str) -> Result<serde_json::Value>,
) -> Result<()> {
    run_support_shutdown_inner(config, &load_original)
}

fn run_support_shutdown_inner(
    config: &SupportShutdownConfig<'_>,
    load_original: &OriginalObservation<'_>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let tunnel = config.output.join("cloudflared");
    let shell = config.output.join("fixture-shell");
    fs::write(
        &tunnel,
        "#!/bin/sh\nprintf \"%s\\n\" \"$$\" > \"$CARROT_OWNED_TUNNEL_ENTERED\"\nwhile ! test -f \"$CARROT_OWNED_TUNNEL_RELEASE\"; do sleep 0.1; done\nprintf \"https://owned-fixture.trycloudflare.com\\n\"\nexec sleep 600\n",
    )?;
    fs::write(&shell, "#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n")?;
    fs::set_permissions(&tunnel, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700))?;
    let created_tunnel = CreatedProgramGuard::record(
        &tunnel,
        &config.output.join("created-shutdown-cloudflared.jsonl"),
    )?;
    let created_shell =
        CreatedProgramGuard::record(&shell, &config.output.join("created-shutdown-shell.jsonl"))?;
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
        let tunnel_pid = wait_for_published_pid(&entered, Duration::from_secs(5))?;
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
        {
            let load = load_original;
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
                    || row["tunnel_pid"].as_u64().is_none_or(|pid| pid == 0)
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
        created_tunnel.finish()?;
        created_shell.finish()?;
        fs::write(
            config.output.join("result.json"),
            serde_json::to_vec_pretty(
                &json!({"pass":true,"exited_within_three_seconds":true,"candidate_sha256":candidate.digest(),"frozen_original_compared":true}),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, RecvTimeoutError};

    #[test]
    fn pid_publication_waits_for_complete_line() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("entered");
        fs::write(&path, "").unwrap();
        let reader_path = path.clone();
        let (send, receive) = mpsc::channel();
        let reader = thread::spawn(move || {
            send.send(
                wait_for_published_pid(&reader_path, Duration::from_secs(5))
                    .map_err(|error| error.to_string()),
            )
            .unwrap();
        });
        assert_eq!(
            receive.recv_timeout(Duration::from_millis(30)),
            Err(RecvTimeoutError::Timeout)
        );
        fs::write(&path, "12").unwrap();
        assert_eq!(
            receive.recv_timeout(Duration::from_millis(30)),
            Err(RecvTimeoutError::Timeout)
        );
        fs::write(&path, "123\n").unwrap();
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok(123)
        );
        reader.join().unwrap();
    }

    #[test]
    fn pid_publication_deadline_and_malformed_controls() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("entered");
        for content in [None, Some(""), Some("12")] {
            if let Some(content) = content {
                fs::write(&path, content).unwrap();
            }
            let error = wait_for_published_pid(&path, Duration::ZERO).unwrap_err();
            assert!(error.to_string().contains("did not publish a complete PID"));
        }
        for content in ["0\n", "not-a-pid\n", "123\n456\n", "4294967296\n"] {
            fs::write(&path, content).unwrap();
            assert!(wait_for_published_pid(&path, Duration::ZERO).is_err());
        }
        fs::write(&path, "123\n").unwrap();
        assert_eq!(wait_for_published_pid(&path, Duration::ZERO).unwrap(), 123);
    }
}
