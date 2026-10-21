use super::{encode_frame, HttpPeer, OwnedServer, Pin, Result, WebSocketPeer};
use crate::recovery_exec_audit::CreatedProgramGuard;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::bytes::Regex;
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    ffi::OsString, fs, net::TcpListener, os::unix::fs::PermissionsExt, path::Path, time::Duration,
};

pub struct TerminalFrozenConfig<'a> {
    pub repository: &'a Path,
    pub source_server: &'a Path,
    pub candidate: &'a Path,
    pub output: &'a Path,
}

fn port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

pub(super) fn marker_pid(bytes: &[u8], marker: &[u8], suffix: &[u8]) -> Result<u32> {
    for index in bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(index, window)| (window == marker).then_some(index))
    {
        let start = index + marker.len();
        let digits = bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .copied()
            .collect::<Vec<_>>();
        if !digits.is_empty()
            && bytes.get(start + digits.len()..start + digits.len() + suffix.len()) == Some(suffix)
        {
            return Ok(std::str::from_utf8(&digits)?.parse()?);
        }
    }
    Err("missing valid terminal PID marker".into())
}

fn output(peer: &mut WebSocketPeer, marker: &[u8]) -> Result<Vec<u8>> {
    peer.output_until(marker, 2 * 1024 * 1024)
}

fn scenario(port: u16, repository: &Path, directory: &Path) -> Result<Value> {
    // serde_json generates the exact client JSON type/value independently of
    // the recovery implementation; use it for the prebuffered first frame.
    let prebuffer = encode_frame(
        1,
        &serde_json::to_vec(&json!({"type":"input","data":"printf \"PREBUFFER=%s\\n\" 263"}))?,
        true,
        true,
    );
    let mut ws = WebSocketPeer::connect(port, "/ws/terminal_pty", &prebuffer)?;
    let meta = ws.event("meta")?;
    if meta
        != json!({"type":"meta","mode":"pty","session":"recovery-shell","created":true,"user":"comma","rows":30,"cols":100})
    {
        return Err("terminal initial metadata changed".into());
    }
    let mut captured = output(&mut ws, b"PREBUFFER=263\r\n")?;
    ws.send_json(&json!({"type":"input","data":"printf \"PID=%s\\n\" \"$$\"; printf \"ENV=%s,%s,%s\\n\" \"$TERM\" \"$COLORTERM\" \"${TMUX-unset}\"; stty size; printf \"CWD=%s\\n\" \"$PWD\""}))?;
    captured.extend_from_slice(&output(&mut ws, b"OWNED> ")?);
    let expected_cwd = format!("CWD={}\r\n", repository.join("openpilot").display());
    for _ in 0..16 {
        if captured
            .windows(expected_cwd.len())
            .any(|window| window == expected_cwd.as_bytes())
            && captured
                .windows(b"ENV=xterm-256color,truecolor,unset".len())
                .any(|window| window == b"ENV=xterm-256color,truecolor,unset")
            && captured.windows(8).any(|window| window == b"30 100\r\n")
            && captured.windows(4).any(|window| window == b"PID=")
        {
            break;
        }
        captured.extend_from_slice(&output(&mut ws, b"OWNED> ")?);
    }
    let pid = marker_pid(&captured, b"PID=", b"\r\n")?;
    if !captured
        .windows(expected_cwd.len())
        .any(|window| window == expected_cwd.as_bytes())
        || !captured
            .windows(b"ENV=xterm-256color,truecolor,unset\r\n".len())
            .any(|window| window == b"ENV=xterm-256color,truecolor,unset\r\n")
        || !captured.windows(8).any(|window| window == b"30 100\r\n")
    {
        return Err("terminal shell environment changed".into());
    }
    let fragment = serde_json::to_vec(&json!({"type":"input","data":"printf \"FRAG=%s\\n\" 263"}))?;
    ws.send_frame(1, &fragment[..20], false, true)?;
    ws.send_frame(9, b"probe263", true, true)?;
    ws.send_frame(10, b"ignored", true, true)?;
    ws.send_frame(0, &fragment[20..], true, true)?;
    loop {
        let frame = ws.frame()?;
        if frame.opcode == 10 {
            if frame.payload != b"probe263" {
                return Err("terminal pong mismatch".into());
            }
            break;
        }
    }
    output(&mut ws, b"FRAG=263\r\n")?;
    ws.send_frame(
        2,
        &serde_json::to_vec(&json!({"type":"raw","data":"printf \"RAW=%s\\n\" 263\r"}))?,
        true,
        false,
    )?;
    output(&mut ws, b"RAW=263\r\n")?;
    let number_row = Regex::new(r"(?m)(?:^|\r\n)\s*(\d+(?:\s+\d+)*)\r\n")?;
    for (action, expected) in [
        ("ctrl_c", &[3_u8][..]),
        ("refresh", &[12_u8][..]),
        ("detach", &[96_u8, 100][..]),
    ] {
        let ready = format!("CONTROL_READY_{action}");
        let done = format!("CONTROL_DONE_{action}");
        let command = format!("stty -icanon -echo -isig min 1 time 0; printf \"{ready}=%s\\n\" 263; od -An -tu1 -N{}; stty sane; printf \"{done}=%s\\n\" 263", expected.len());
        ws.send_json(&json!({"type":"input","data":command}))?;
        output(&mut ws, format!("{ready}=263\r\n").as_bytes())?;
        ws.send_json(&json!({"type":"control","action":action}))?;
        let observed = output(&mut ws, format!("{done}=263\r\n").as_bytes())?;
        let matched = number_row.captures_iter(&observed).any(|capture| {
            capture.get(1).is_some_and(|row| {
                row.as_bytes()
                    .split(|byte| byte.is_ascii_whitespace())
                    .filter(|part| !part.is_empty())
                    .map(|part| {
                        std::str::from_utf8(part)
                            .ok()
                            .and_then(|number| number.parse::<u8>().ok())
                    })
                    .collect::<Option<Vec<_>>>()
                    .is_some_and(|numbers| numbers == expected)
            })
        });
        if !matched {
            return Err(format!("terminal control bytes mismatch: {action}").into());
        }
    }
    ws.send_json(&json!({"type":"resize","rows":222,"cols":500}))?;
    let resize = ws.event("pty_resize")?;
    if resize != json!({"type":"pty_resize","session":"recovery-shell","rows":200,"cols":100}) {
        return Err("terminal resize clamp mismatch".into());
    }
    ws.send_json(&json!({"type":"input","data":"stty size; printf \"RESIZED=%s\\n\" 263"}))?;
    if !output(&mut ws, b"RESIZED=263\r\n")?
        .windows(9)
        .any(|window| window == b"200 100\r\n")
    {
        return Err("terminal PTY geometry mismatch".into());
    }
    ws.send_json(&json!({"type":"input","data":"persistent_value=263; printf \"REMEMBER=%s\\n\" \"$persistent_value\""}))?;
    output(&mut ws, b"REMEMBER=263\r\n")?;
    ws.close()?;
    drop(ws);
    let mut ws = WebSocketPeer::connect(port, "/ws/terminal_pty", &[])?;
    let reconnect = ws.event("meta")?;
    let replay = ws.event("pty_output")?;
    let replay_bytes = STANDARD.decode(
        replay
            .get("b64")
            .and_then(Value::as_str)
            .ok_or("missing terminal replay")?,
    )?;
    if reconnect.get("created") != Some(&Value::Bool(false))
        || reconnect.get("rows") != Some(&Value::from(200))
        || replay.get("replay") != Some(&Value::Bool(true))
        || !replay_bytes
            .windows(14)
            .any(|window| window == b"REMEMBER=263\r\n")
    {
        return Err("terminal reconnect replay mismatch".into());
    }
    ws.send_json(
        &json!({"type":"input","data":"printf \"PERSIST=%s,%s\\n\" \"$$\" \"$persistent_value\""}),
    )?;
    output(&mut ws, format!("PERSIST={pid},263\r\n").as_bytes())?;
    ws.send_json(&json!({"type":"input","data":"printf \"OLD_HISTORY=%s\\n\" 263"}))?;
    output(&mut ws, b"OLD_HISTORY=263\r\n")?;
    ws.send_json(&json!({"type":"control","action":"clear"}))?;
    ws.send_json(&json!({"type":"input","data":"printf \"AFTER_CLEAR=%s\\n\" 263"}))?;
    output(&mut ws, b"AFTER_CLEAR=263\r\n")?;
    let mut second = WebSocketPeer::connect(port, "/ws/terminal_pty", &[])?;
    if second.event("meta")?.get("created") != Some(&Value::Bool(false)) {
        return Err("terminal second client created new shell".into());
    }
    let cleared = second.event("pty_output")?;
    let cleared_bytes = STANDARD.decode(
        cleared
            .get("b64")
            .and_then(Value::as_str)
            .ok_or("missing cleared history")?,
    )?;
    if cleared.get("replay") != Some(&Value::Bool(true))
        || cleared_bytes
            .windows(17)
            .any(|window| window == b"OLD_HISTORY=263\r\n")
        || !cleared_bytes
            .windows(17)
            .any(|window| window == b"AFTER_CLEAR=263\r\n")
    {
        return Err("terminal history clear mismatch".into());
    }
    ws.send_json(&json!({"type":"input","data":"printf \"BROADCAST=%s\\n\" 263"}))?;
    output(&mut ws, b"BROADCAST=263\r\n")?;
    output(&mut second, b"BROADCAST=263\r\n")?;
    ws.send_json(&json!({"type":"input","data":"head -c 600000 /dev/zero | tr \"\\0\" X; printf \"BIG_END=%s\\n\" 263"}))?;
    output(&mut ws, b"BIG_END=263\r\n")?;
    let mut large = WebSocketPeer::connect(port, "/ws/terminal_pty", &[])?;
    if large.event("meta")?.get("created") != Some(&Value::Bool(false)) {
        return Err("terminal history reader created new shell".into());
    }
    let history = large.event("pty_output")?;
    let history_bytes = STANDARD.decode(
        history
            .get("b64")
            .and_then(Value::as_str)
            .ok_or("missing bounded history")?,
    )?;
    fs::write(directory.join("bounded-history.bin"), &history_bytes)?;
    if history.get("replay") != Some(&Value::Bool(true))
        || history_bytes.len() != 512 * 1024
        || !history_bytes
            .windows(13)
            .any(|window| window == b"BIG_END=263\r\n")
        || history_bytes
            .windows(15)
            .any(|window| window == b"BROADCAST=263\r\n")
    {
        return Err("terminal history size or contents mismatch".into());
    }
    ws.send_json(&json!({"type":"input","data":"exit 7"}))?;
    for peer in [&mut ws, &mut second, &mut large] {
        if peer.event("pty_exit")?.get("exit_code") != Some(&Value::from(7))
            || peer.frame()?.opcode != 8
        {
            return Err("terminal PTY exit broadcast mismatch".into());
        }
    }
    drop(ws);
    drop(second);
    drop(large);
    let mut fresh = WebSocketPeer::connect(port, "/ws/terminal_pty?reset=1", &[])?;
    if fresh.event("meta")?.get("created") != Some(&Value::Bool(true)) {
        return Err("terminal reset did not create shell".into());
    }
    fresh.send_json(&json!({"type":"input","data":"printf \"NEW=%s,%s\\n\" \"$$\" \"${persistent_value-unset}\""}))?;
    let newer = output(&mut fresh, b",unset\r\n")?;
    let fresh_pid = marker_pid(&newer, b"NEW=", b",unset\r\n")?;
    if fresh_pid == pid {
        return Err("terminal reset reused shell PID".into());
    }
    fresh.send_json(&json!({"type":"input","data":"exit 0"}))?;
    if fresh.event("pty_exit")?.get("exit_code") != Some(&Value::from(0)) {
        return Err("terminal fresh shell exit mismatch".into());
    }
    Ok(
        json!({"meta":meta,"environment":"xterm-256color,truecolor,unset","initial_geometry":[30,100],"source_repository_cwd":true,"prebuffered_frame":true,"fragmented_ping_pong":true,"unmasked_binary_raw":true,"control_ctrl_c_pty_bytes":[3],"control_refresh_pty_bytes":[12],"control_detach_pty_bytes":[96,100],"resize":resize,"reconnect_persistent_pid_and_variable":true,"history_replay":true,"clear_history":true,"two_client_broadcast":true,"bounded_history_bytes":history_bytes.len(),"exit_code_and_client_close":7,"new_shell_after_exit":true,"shell_pid":pid,"fresh_pid":fresh_pid}),
    )
}

pub fn run_terminal_frozen(
    config: &TerminalFrozenConfig<'_>,
    load_original: impl Fn(&str) -> Result<Value>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let source_pin = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let candidate_pin = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let source_proof = load_original("terminal/source/source-proof.json")?;
    let source_cleanup = load_original("terminal/source/cleanup.json")?;
    let source_invocation = load_original("terminal/source/invocation.json")?;
    let source_wire = load_original("terminal/source/wire.json")?;
    let mut source_result = load_original("terminal/source/result.json")?;
    if source_proof["sha256"].as_str() != Some(source_pin.digest())
        || source_proof["source_main_cleanup_not_claimed"] != true
        || source_cleanup["exit_code"] != 0
        || source_wire.as_array().is_none_or(|rows| rows.len() != 395)
        || !source_invocation["command"]
            .as_array()
            .is_some_and(|command| {
                command.len() == 7
                    && command[2]
                        .as_str()
                        .is_some_and(|path| path.ends_with("/rust/tools/carrot_recovery_source.py"))
            })
    {
        return Err("frozen terminal source provenance changed".into());
    }
    let source_pid = source_result["pid"]
        .as_u64()
        .ok_or("original terminal PID absent")?;
    let source_final_pid = source_result["final_pid"]
        .as_u64()
        .ok_or("original terminal final PID absent")?;
    if source_pid == 0 || source_final_pid == 0 || source_pid == source_final_pid {
        return Err("original terminal PID relation changed".into());
    }
    source_result
        .as_object_mut()
        .ok_or("original terminal result not an object")?
        .remove("pid");
    source_result
        .as_object_mut()
        .ok_or("original terminal result not an object")?
        .remove("final_pid");
    let directory = config.output.join("native");
    fs::create_dir(&directory)?;
    let shell = directory.join("fixture-shell");
    fs::write(&shell, "#!/bin/sh\nprintf \"%s\\n\" \"$@\" >> \"$CARROT_RECOVERY_SHELL_LOG\"\nexec /bin/bash --noprofile --norc -i\n")?;
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700))?;
    let created_shell =
        CreatedProgramGuard::record(&shell, &config.output.join("created-terminal-shell.jsonl"))?;
    let port = port()?;
    let argv = [
        OsString::from(config.candidate),
        OsString::from("--host"),
        OsString::from("127.0.0.1"),
        OsString::from("--port"),
        port.to_string().into(),
    ];
    let environment = [
        (OsString::from("USER"), OsString::from("comma")),
        (OsString::from("SHELL"), OsString::from(&shell)),
        (OsString::from("PS1"), OsString::from("OWNED> ")),
        (OsString::from("PS2"), OsString::from("CONT> ")),
        (OsString::from("TMUX"), OsString::from("must-be-removed")),
        (
            OsString::from("CARROT_RECOVERY_SHELL_LOG"),
            OsString::from(directory.join("shell-argv.log")),
        ),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        config.repository,
        &directory.join("process.log"),
        &environment,
        &["TERM", "COLORTERM"],
        false,
    )?;
    let peer = HttpPeer::new(port, Duration::from_secs(2));
    let result = (|| -> Result<Value> {
        peer.wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        scenario(port, config.repository, &directory)
    })();
    let stop = server.signal(Signal::TERM);
    let exit = server.finish(Duration::from_secs(5));
    fs::write(
        directory.join("cleanup.json"),
        serde_json::to_vec_pretty(&json!({"stop":stop.is_ok(),"exit":exit.as_ref().ok()}))?,
    )?;
    let mut native_result = result?;
    stop?;
    if exit? != 0 {
        return Err("native frozen terminal server cleanup failed".into());
    }
    let shell_pid = native_result["shell_pid"]
        .as_u64()
        .ok_or("native terminal PID absent")?;
    let fresh_pid = native_result["fresh_pid"]
        .as_u64()
        .ok_or("native terminal fresh PID absent")?;
    if shell_pid == 0 || fresh_pid == 0 || shell_pid == fresh_pid {
        return Err("native terminal PID relation changed".into());
    }
    native_result
        .as_object_mut()
        .ok_or("native terminal result not an object")?
        .remove("shell_pid");
    native_result
        .as_object_mut()
        .ok_or("native terminal result not an object")?
        .remove("fresh_pid");
    let original_argv = super::original_fixture::OriginalFixture::open()?
        .opaque_payload("terminal/source/shell-argv.log.json")?;
    require_shell_argv(&original_argv, &fs::read(directory.join("shell-argv.log"))?)?;
    if source_result != native_result {
        return Err("frozen original/native terminal scenario changed".into());
    }
    source_pin.verify()?;
    candidate_pin.verify()?;
    created_shell.finish()?;
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"frozen_original_compared":true,
            "comparison":native_result,"source_sha256":source_pin.digest(),
            "candidate_sha256":candidate_pin.digest()}))?,
    )?;
    Ok(())
}

fn require_shell_argv(original: &[u8], native: &[u8]) -> Result<()> {
    if original.is_empty() || original != native {
        return Err("frozen terminal shell bootstrap argv changed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::marker_pid;

    #[test]
    fn echoed_command_is_not_mistaken_for_actual_pid() {
        let output = b"printf \"ROUND=%s,0\\n\" \"$$\"\r\nROUND=413,0\r\n";
        assert_eq!(marker_pid(output, b"ROUND=", b",0\r\n").unwrap(), 413);
        assert!(marker_pid(b"ROUND=unresolved\r\n", b"ROUND=", b"\r\n").is_err());
    }
}

#[cfg(test)]
mod frozen_argv_tests {
    use super::*;
    #[test]
    fn nonempty_changed_bootstrap_argv_does_not_match_original() {
        assert!(require_shell_argv(b"-l\n-i\n", b"-i\n").is_err());
        assert!(require_shell_argv(b"-l\n-i\n", b"-i\n-l\n").is_err());
        assert!(require_shell_argv(b"", b"").is_err());
        assert!(require_shell_argv(b"-l\n-i\n", b"-l\n-i\n").is_ok());
    }
}
