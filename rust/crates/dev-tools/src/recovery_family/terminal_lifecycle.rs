use super::{terminal_control::marker_pid, HttpPeer, OwnedServer, Pin, Result, WebSocketPeer};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub struct TerminalLifecycleConfig<'a> {
    pub repository: &'a Path,
    pub candidate: &'a Path,
    pub output: &'a Path,
}

fn process_state(pid: u32) -> Result<Option<char>> {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => Ok(Some(
            stat.rsplit_once(')')
                .ok_or("invalid owned PTY stat")?
                .1
                .trim_start()
                .chars()
                .next()
                .ok_or("missing owned PTY state")?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn await_disappearance(pid: u32) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if process_state(pid)?.is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("owned PTY descendant {pid} survived cleanup").into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn output(peer: &mut WebSocketPeer, marker: &[u8]) -> Result<Vec<u8>> {
    peer.output_until(marker, 1024 * 1024)
}

fn output_with_pid(
    peer: &mut WebSocketPeer,
    marker: &[u8],
    prefix: &[u8],
    suffix: &[u8],
) -> Result<(Vec<u8>, u32)> {
    let mut bytes = output(peer, marker)?;
    for _ in 0..16 {
        if let Ok(pid) = marker_pid(&bytes, prefix, suffix) {
            return Ok((bytes, pid));
        }
        let row = peer.event("pty_output")?;
        bytes.extend_from_slice(
            &STANDARD.decode(
                row.get("b64")
                    .and_then(Value::as_str)
                    .ok_or("missing PTY output")?,
            )?,
        );
    }
    Err("missing bounded PTY PID response".into())
}

/// Native lifecycle counterpart to the unchanged terminal_lifecycle.py's
/// strict side. The original source-race observation continues in the old
/// controller until a fresh independent capture is sealed.
pub fn run_terminal_lifecycle(config: &TerminalLifecycleConfig<'_>) -> Result<()> {
    run_terminal_lifecycle_inner(config, None)
}

/// Compare the fresh native reset and cleanup effects with immutable original
/// observations. Process IDs are required to be distinct within each run,
/// rather than copied from the historical host.
pub fn run_terminal_lifecycle_frozen(
    config: &TerminalLifecycleConfig<'_>,
    load_original: impl Fn(&str) -> Result<Value>,
) -> Result<()> {
    run_terminal_lifecycle_inner(config, Some(&load_original))
}

fn run_terminal_lifecycle_inner(
    config: &TerminalLifecycleConfig<'_>,
    load_original: Option<&dyn Fn(&str) -> Result<Value>>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let shell = config.output.join("fixture-shell");
    fs::write(&shell, "#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n")?;
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700))?;
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        OsString::from(config.candidate),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string().into(),
    ];
    let environment = [
        (OsString::from("USER"), OsString::from("comma")),
        (OsString::from("SHELL"), OsString::from(&shell)),
        (OsString::from("PS1"), OsString::from("OWNED> ")),
        (OsString::from("PS2"), OsString::from("CONT> ")),
        (
            OsString::from("CARROT_RECOVERY_LIFECYCLE_ID"),
            OsString::from(config.output),
        ),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        config.repository,
        &config.output.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let result = (|| -> Result<(Vec<u32>, u32, u32)> {
        HttpPeer::new(port, Duration::from_secs(2))
            .wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        let mut pids = Vec::new();
        let mut rows = Vec::new();
        for round in 0..24_u8 {
            let mut peer = WebSocketPeer::connect(port, "/ws/terminal_pty?reset=1", &[])?;
            let meta = peer.event("meta")?;
            if meta.get("created") != Some(&Value::Bool(true))
                || meta.get("rows") != Some(&Value::from(30))
            {
                return Err(format!("native PTY reset metadata failed at round {round}").into());
            }
            peer.send_json(
                &json!({"type":"input","data":format!("printf \"ROUND=%s,{round}\\n\" \"$$\"")}),
            )?;
            let marker = format!(",{round}\r\n");
            let (bytes, pid) =
                output_with_pid(&mut peer, marker.as_bytes(), b"ROUND=", marker.as_bytes())?;
            if !bytes
                .windows(marker.len())
                .any(|window| window == marker.as_bytes())
            {
                return Err("native PTY reset round number mismatch".into());
            }
            pids.push(pid);
            rows.push(json!({"round":round,"pid":pid,"command_completed":true}));
            peer.close()?;
        }
        fs::write(
            config.output.join("rounds.json"),
            serde_json::to_vec_pretty(&rows)?,
        )?;
        let mut previous = WebSocketPeer::connect(port, "/ws/terminal_pty?reset=1", &[])?;
        if previous.event("meta")?.get("created") != Some(&Value::Bool(true)) {
            return Err("native lifecycle previous PTY not created".into());
        }
        let mut peer = WebSocketPeer::connect(port, "/ws/terminal_pty?reset=1", &[])?;
        if peer.event("meta")?.get("created") != Some(&Value::Bool(true))
            || previous.event("pty_exit")?
                != json!({"type":"pty_exit","session":"recovery-shell","exit_code":null})
            || previous.frame()?.opcode != 8
        {
            return Err("native lifecycle active reader not closed before reset".into());
        }
        drop(previous);
        let script = "trap \"\" HUP; sleep 600 & printf \"GRANDCHILD=%s\\n\" \"$!\"; wait";
        peer.send_json(&json!({"type":"input","data":format!("/bin/sh -c '{}' & printf \"CHILD=%s\\n\" \"$!\"; printf \"LEADER=%s\\n\" \"$$\"", script.replace('\'', "'\\''"))}))?;
        let (mut bytes, leader) = output_with_pid(&mut peer, b"OWNED> ", b"LEADER=", b"\r\n")?;
        for _ in 0..16 {
            if marker_pid(&bytes, b"CHILD=", b"\r\n").is_ok()
                && marker_pid(&bytes, b"GRANDCHILD=", b"\r\n").is_ok()
            {
                break;
            }
            let row = peer.event("pty_output")?;
            bytes.extend_from_slice(
                &STANDARD.decode(
                    row.get("b64")
                        .and_then(Value::as_str)
                        .ok_or("missing lifecycle output")?,
                )?,
            );
        }
        let child = marker_pid(&bytes, b"CHILD=", b"\r\n")?;
        let grandchild = marker_pid(&bytes, b"GRANDCHILD=", b"\r\n")?;
        let child_before = process_state(child)?;
        peer.send_json(&json!({"type":"input","data":"exit 9"}))?;
        if peer.event("pty_exit")?.get("exit_code") != Some(&Value::from(9)) {
            return Err("native PTY leader exit code changed".into());
        }
        for pid in [child, leader, grandchild] {
            await_disappearance(pid)?;
        }
        for &pid in &pids {
            await_disappearance(pid)?;
        }
        fs::write(
            config.output.join("descendants.json"),
            serde_json::to_vec_pretty(
                &json!({"leader":leader,"child":child,"grandchild":grandchild,"child_before":child_before,"all_absent_after":true}),
            )?,
        )?;
        drop(peer);
        let mut shutdown = WebSocketPeer::connect(port, "/ws/terminal_pty?reset=1", &[])?;
        if shutdown.event("meta")?.get("created") != Some(&Value::Bool(true)) {
            return Err("native shutdown PTY not created".into());
        }
        shutdown.send_json(&json!({"type":"input","data":"/bin/sh -c 'trap \"\" HUP; exec sleep 600' & printf \"SHUTDOWN_CHILD=%s\\n\" \"$!\"; printf \"SHUTDOWN_LEADER=%s\\n\" \"$$\""}))?;
        let (mut bytes, shutdown_leader) =
            output_with_pid(&mut shutdown, b"OWNED> ", b"SHUTDOWN_LEADER=", b"\r\n")?;
        for _ in 0..16 {
            if marker_pid(&bytes, b"SHUTDOWN_CHILD=", b"\r\n").is_ok() {
                break;
            }
            let row = shutdown.event("pty_output")?;
            bytes.extend_from_slice(
                &STANDARD.decode(
                    row.get("b64")
                        .and_then(Value::as_str)
                        .ok_or("missing shutdown output")?,
                )?,
            );
        }
        let shutdown_child = marker_pid(&bytes, b"SHUTDOWN_CHILD=", b"\r\n")?;
        Ok((pids, shutdown_child, shutdown_leader))
    })();
    let stop = server.signal(Signal::TERM);
    let exit_observed = server.wait_unreaped(Duration::from_secs(5));
    let cleanup_observation = if let Ok((_, child, leader)) = &result {
        Some((await_disappearance(*child), await_disappearance(*leader)))
    } else {
        None
    };
    fs::write(
        config.output.join("cleanup.json"),
        serde_json::to_vec_pretty(
            &json!({"stop":stop.is_ok(),"exit_observed":exit_observed.as_ref().ok(),"descendants_absent_before_owner_cleanup":cleanup_observation.as_ref().map(|(child,leader)|[child.is_ok(),leader.is_ok()])}),
        )?,
    )?;
    let exit = server.finish(Duration::from_secs(1));
    let (pids, child, leader) = result?;
    stop?;
    let descendants_before_owner_cleanup = match cleanup_observation {
        Some((child_state, leader_state)) => child_state.is_ok() && leader_state.is_ok(),
        None => false,
    };
    if exit_observed? != 0
        || exit? != 0
        || !descendants_before_owner_cleanup
        || process_state(child)?.is_some()
        || process_state(leader)?.is_some()
    {
        return Err("native lifecycle server or descendant cleanup failed".into());
    }
    if let Some(load) = load_original {
        let original = load("lifecycle/source/result.json")?;
        let original_rounds = original["reset_rounds"]
            .as_array()
            .ok_or("original lifecycle reset rounds absent")?;
        let native_rows: Value =
            serde_json::from_slice(&fs::read(config.output.join("rounds.json"))?)?;
        let native_rounds = native_rows
            .as_array()
            .ok_or("native lifecycle reset rounds absent")?;
        if original_rounds.len() != 24
            || native_rounds.len() != 24
            || original["source_race_observed"] != false
        {
            return Err("original/native lifecycle roster changed".into());
        }
        let mut original_pids = std::collections::BTreeSet::new();
        let mut native_pids = std::collections::BTreeSet::new();
        for (index, (source, native)) in original_rounds.iter().zip(native_rounds).enumerate() {
            let source_pid = source["pid"].as_u64().ok_or("original reset PID absent")?;
            let native_pid = native["pid"].as_u64().ok_or("native reset PID absent")?;
            if source["round"].as_u64() != Some(index as u64)
                || native["round"].as_u64() != Some(index as u64)
                || source["command_completed"] != true
                || native["command_completed"] != true
                || source_pid == 0
                || native_pid == 0
                || !original_pids.insert(source_pid)
                || !native_pids.insert(native_pid)
            {
                return Err(format!("lifecycle reset relation changed: {index}").into());
            }
        }
        let source_cleanup = load("lifecycle/source/cleanup.json")?;
        if source_cleanup["exit_code"] != 0
            || source_cleanup["source_fixture_leftovers_removed"] != json!([])
        {
            return Err("original lifecycle cleanup changed".into());
        }
        let source_wire = load("lifecycle/source/wire.json")?;
        if source_wire.as_array().is_none_or(|rows| rows.len() != 156) {
            return Err("original lifecycle wire roster changed".into());
        }
    }
    candidate.verify()?;
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"pass":true,"native_24_reset_rounds":pids.len()==24,"native_active_peer_closed_before_replacement":true,"native_leader_and_two_level_descendant_cleanup":true,"native_server_shutdown_cleanup":true,"candidate_sha256":candidate.digest(),"frozen_original_compared":load_original.is_some(),"source_race_observation":"historical source-only observation; not native success expectation"}),
        )?,
    )?;
    Ok(())
}
