use super::{HttpPeer, OwnedServer, Pin, Reply, Result, ScriptedHttpPeer, WebSocketPeer};
use crate::recovery_exec_audit::CreatedProgramGuard;
use regex::Regex;
use rustix::process::Signal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    net::{TcpListener, TcpStream},
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub struct SupportFrozenConfig<'a> {
    pub repository: &'a Path,
    pub source_server: &'a Path,
    pub candidate: &'a Path,
    pub fixture_binary: &'a Path,
    pub process_child: &'a Path,
    pub output: &'a Path,
}

type HttpObservation = (u16, Value, Vec<u8>, Vec<(String, String)>);

struct Session {
    id: String,
    port: u16,
    pin: String,
}

fn request(
    peer: &HttpPeer,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<HttpObservation> {
    let body = body
        .map(serde_json::to_vec)
        .transpose()?
        .unwrap_or_default();
    let response = peer.request(method, path, &body)?;
    let parsed = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    Ok((response.status, parsed, response.body, response.headers))
}

fn check(ok: bool, message: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn ws(port: u16, path: &str) -> Result<WebSocketPeer> {
    WebSocketPeer::connect(port, path, &[])
}

fn request_rows(peer: &ScriptedHttpPeer, count: usize) -> Result<Vec<super::CapturedRequest>> {
    peer.wait_requests(count, Duration::from_secs(5))
}

fn start(
    peer: &HttpPeer,
    webhook: &ScriptedHttpPeer,
    ordinal: usize,
    mode: &str,
) -> Result<Session> {
    let (status, session, _, _) = request(
        peer,
        "POST",
        "/api/support_terminal/start",
        Some(
            &json!({"note":"owned support scenario","permission_mode":mode,"ttl_seconds":900,"command_timeout_seconds":15}),
        ),
    )?;
    check(
        status == 200
            && session["active"] == true
            && session["state"] == "sharing"
            && session["permission_mode"] == mode
            && session["command_timeout_seconds"] == 15
            && session["ttl_seconds"] == 900,
        "support start contract changed",
    )?;
    let rows = request_rows(webhook, ordinal)?;
    let row = &rows[ordinal - 1];
    check(
        row.method == "POST" && row.path == "/owned-webhook",
        "support webhook route changed",
    )?;
    let payload: Value = serde_json::from_slice(&row.body)?;
    let content = payload["content"]
        .as_str()
        .ok_or("missing support webhook content")?;
    let expression = Regex::new(r"PIN: \*\*__(\d{6})__\*\*")?;
    let pin = expression
        .captures(content)
        .and_then(|capture| capture.get(1))
        .ok_or("missing support webhook PIN")?
        .as_str()
        .to_owned();
    let id = session["id"]
        .as_str()
        .ok_or("missing support session id")?
        .to_owned();
    check(!id.is_empty(), "empty support session id")?;
    let url = session["url"].as_str().ok_or("missing support URL")?;
    check(
        content.contains(&id)
            && content.contains(url)
            && payload["allowed_mentions"] == json!({"parse":[]}),
        "support webhook identity changed",
    )?;
    for expected in [
        "Car name: owned-support-car",
        "DongleId: owned-support-id",
        "Serial: owned-support-serial",
    ] {
        check(content.contains(expected), "support webhook Params changed")?;
    }
    let local = session["local_url"]
        .as_str()
        .ok_or("missing support local URL")?;
    let port: u16 = local
        .strip_prefix("http://127.0.0.1:")
        .ok_or("support URL escaped loopback")?
        .split('/')
        .next()
        .ok_or("missing support URL port")?
        .parse()?;
    Ok(Session { id, port, pin })
}

fn guest(session: &Session, granted: bool) -> Result<WebSocketPeer> {
    let mut peer = ws(
        session.port,
        &format!("/ws/support_terminal/{}", session.id),
    )?;
    peer.send_json(&json!({"type":"auth","pin":session.pin}))?;
    check(
        peer.event("auth_ok")?["control_granted"] == granted
            && peer.event("meta")?["session"] == "recovery-shell",
        "support guest authorization changed",
    )?;
    Ok(peer)
}

fn owner(port: u16) -> Result<WebSocketPeer> {
    let mut peer = ws(port, "/ws/support_terminal/owner")?;
    check(
        peer.event("session_status")?["owner_present"] == true,
        "support owner connection changed",
    )?;
    Ok(peer)
}

fn stop(peer: &HttpPeer, active: &mut [&mut WebSocketPeer], guest_port: u16) -> Result<()> {
    let (status, value, _, _) =
        request(peer, "POST", "/api/support_terminal/stop", Some(&json!({})))?;
    check(
        status == 200 && value == json!({"ok":true,"active":false,"state":"idle"}),
        "support stop result changed",
    )?;
    for client in active {
        check(
            client.event("session_closed")?["reason"] == "user" && client.frame()?.opcode == 8,
            "support stop did not close peer",
        )?;
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while TcpStream::connect(("127.0.0.1", guest_port)).is_ok() {
        if Instant::now() >= deadline {
            return Err("support guest listener survived stop".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn exercise(port: u16, directory: &Path, webhook: &ScriptedHttpPeer) -> Result<Value> {
    let peer = HttpPeer::new(port, Duration::from_secs(8));
    let mut static_rows = Vec::new();
    let mut idle = ws(port, "/ws/support_terminal/owner")?;
    check(
        idle.event("session_status")?
            == json!({"type":"session_status","ok":true,"active":false,"state":"idle"})
            && idle.frame()?.opcode == 8,
        "idle support owner did not close",
    )?;
    drop(idle);
    let first = start(&peer, webhook, 1, "approve_each")?;
    let (status, duplicate, _, _) = request(
        &peer,
        "POST",
        "/api/support_terminal/start",
        Some(&json!({"permission_mode":"allow_all","note":"duplicate"})),
    )?;
    check(
        status == 200
            && duplicate["id"].as_str() == Some(first.id.as_str())
            && duplicate["permission_mode"] == "approve_each",
        "support duplicate start replaced session",
    )?;
    let guest_http = HttpPeer::new(first.port, Duration::from_secs(8));
    for path in [
        format!("/support/terminal/{}", first.id),
        "/support-terminal-assets/xterm.js".into(),
        "/api/support_terminal/status".into(),
        "/api/terminal/input".into(),
        "/download/tmux.log".into(),
    ] {
        let (status, _, raw, headers) = request(&guest_http, "GET", &path, None)?;
        let expected = if path.starts_with("/support") {
            200
        } else {
            404
        };
        check(status == expected, "support guest route isolation changed")?;
        if path.starts_with("/support/terminal/") {
            let map: std::collections::BTreeMap<_, _> = headers.into_iter().collect();
            check(
                map.get("x-frame-options")
                    .is_some_and(|value| value == "DENY")
                    && map
                        .get("x-content-type-options")
                        .is_some_and(|value| value == "nosniff")
                    && map.get("content-security-policy").is_some_and(|value| {
                        value.contains("connect-src 'self' ws://owned wss://owned")
                    }),
                "support guest security headers changed",
            )?;
            let mut normalized = Vec::with_capacity(raw.len());
            let needle = first.id.as_bytes();
            let mut rest = raw.as_slice();
            while let Some(index) = rest
                .windows(needle.len())
                .position(|window| window == needle)
            {
                normalized.extend_from_slice(&rest[..index]);
                normalized.extend_from_slice(b"OWNED_SESSION");
                rest = &rest[index + needle.len()..];
            }
            normalized.extend_from_slice(rest);
            let headers: std::collections::BTreeMap<_, _> = map
                .into_iter()
                .filter(|(name, _)| !matches!(name.as_str(), "date" | "server" | "content-length"))
                .collect();
            static_rows.push(json!({"path":"/support/terminal/OWNED_SESSION","sha256":format!("{:x}",Sha256::digest(&normalized)),"headers":headers}));
        } else if status == 200 {
            static_rows.push(json!({"path":path,"sha256":format!("{:x}",Sha256::digest(&raw))}));
        }
    }
    let mut bad = ws(first.port, &format!("/ws/support_terminal/{}", first.id))?;
    let wrong = if first.pin != "000000" {
        "000000"
    } else {
        "999999"
    };
    for remaining in [4, 3, 2, 1, 0] {
        bad.send_json(&json!({"type":"auth","pin":wrong}))?;
        check(
            bad.event("auth_failed")? == json!({"type":"auth_failed","remaining":remaining}),
            "support PIN attempt count changed",
        )?;
    }
    check(
        bad.frame()?.opcode == 8,
        "support bad PIN peer remained open",
    )?;
    drop(bad);
    let mut remote = guest(&first, true)?;
    let denied = directory.join("owner-absent-denied");
    remote.send_json(&json!({"type":"input","data":format!("touch {}",shell_quote(&denied))}))?;
    check(
        remote.event("owner_absent")? == json!({"type":"owner_absent"}) && !denied.exists(),
        "support owner-absent input executed",
    )?;
    remote.send_json(&json!({"type":"raw","data":format!("touch {}\r",shell_quote(&denied))}))?;
    check(
        remote.event("owner_absent")? == json!({"type":"owner_absent"}) && !denied.exists(),
        "support owner-absent raw executed",
    )?;
    let mut host = owner(port)?;
    check(
        remote.event("owner_presence")?["active"] == true,
        "support owner presence absent",
    )?;
    host.send_json(&json!({"type":"typing","active":true,"text":"a\nb\tc"}))?;
    check(
        remote.event("host_typing")? == json!({"type":"host_typing","active":true,"text":"abc"}),
        "support host typing sanitize changed",
    )?;
    remote.send_json(&json!({"type":"typing","active":true,"text":"x\ny\tz"}))?;
    check(
        host.event("guest_typing")? == json!({"type":"guest_typing","active":true,"text":"xyz"}),
        "support guest typing sanitize changed",
    )?;
    // The raw escape retains the original Python lone-surrogate input; Rust
    // String cannot represent a surrogate scalar.
    let advanced = br#"{"type":"typing","active":true,"text":"A\u202eB\u200dC\u0378D\ufeffE\ue000F\ud800G\u00a0 H"}"#;
    host.send_frame(1, advanced, true, true)?;
    check(
        remote.event("host_typing")?
            == json!({"type":"host_typing","active":true,"text":"ABCDEFG H"}),
        "support advanced host typing sanitize changed",
    )?;
    remote.send_frame(1, advanced, true, true)?;
    check(
        host.event("guest_typing")?
            == json!({"type":"guest_typing","active":true,"text":"ABCDEFG H"}),
        "support advanced guest typing sanitize changed",
    )?;
    remote.send_json(&json!({"type":"raw","data":"ignored"}))?;
    check(
        remote.event("input_denied")?
            == json!({"type":"input_denied","reason":"approval required"}),
        "support approval raw denial changed",
    )?;
    let approved = directory.join("approved.txt");
    let line = format!(
        "printf \"263-approved\" > {}; printf \"APPROVED=%s\\n\" 263",
        shell_quote(&approved)
    );
    remote.send_json(&json!({"type":"input","data":line}))?;
    let waiting = remote.event("command_waiting_approval")?;
    let queued = host.event("command_request")?;
    check(
        queued["id"] == waiting["id"] && queued["line"] == line && !approved.exists(),
        "support approval queue changed",
    )?;
    let approval = format!(
        "/api/support_terminal/commands/{}/approve",
        waiting["id"].as_str().ok_or("missing command id")?
    );
    let (status, outcome, _, _) = request(&peer, "POST", &approval, Some(&json!({})))?;
    check(
        status == 200 && outcome == json!({"ok":true}),
        "support approve route changed",
    )?;
    let id = &waiting["id"];
    check(
        remote.event("command_approved")?.get("id") == Some(id)
            && remote.event("command_running")?.get("id") == Some(id),
        "support approved command events changed",
    )?;
    remote.output_until(b"APPROVED=263\r\n", 1024 * 1024)?;
    check(
        fs::read_to_string(&approved)? == "263-approved",
        "support approved shell effect missing",
    )?;
    let rejected = directory.join("rejected.txt");
    remote.send_json(&json!({"type":"input","data":format!("touch {}",shell_quote(&rejected))}))?;
    let pending = remote.event("command_waiting_approval")?;
    check(
        host.event("command_request")?["id"] == pending["id"],
        "support reject queue changed",
    )?;
    let command_id = pending["id"].as_str().ok_or("missing reject command id")?;
    let (status, outcome, _, _) = request(
        &peer,
        "POST",
        &format!("/api/support_terminal/commands/{command_id}/reject"),
        Some(&json!({})),
    )?;
    check(
        status == 200
            && outcome == json!({"ok":true})
            && remote.event("command_rejected")?["id"] == pending["id"],
        "support rejection changed",
    )?;
    let (status, outcome, _, _) = request(
        &peer,
        "POST",
        &format!("/api/support_terminal/commands/{command_id}/approve"),
        Some(&json!({})),
    )?;
    check(
        status == 400
            && outcome == json!({"ok":false,"error":"command not pending"})
            && !rejected.exists(),
        "support rejected command executed",
    )?;
    let expired = directory.join("expired.txt");
    remote.send_json(&json!({"type":"input","data":format!("touch {}",shell_quote(&expired))}))?;
    let pending = remote.event("command_waiting_approval")?;
    check(
        host.event("command_request")?["id"] == pending["id"],
        "support expiry queue changed",
    )?;
    let began = Instant::now();
    remote.set_read_timeout(Duration::from_secs(20))?;
    let expiration = remote.event("command_expired")?;
    remote.set_read_timeout(Duration::from_secs(8))?;
    let elapsed = began.elapsed();
    check(
        expiration["id"] == pending["id"]
            && elapsed >= Duration::from_secs(14)
            && elapsed <= Duration::from_secs(18)
            && !expired.exists(),
        "support command expiry deadline changed",
    )?;
    fs::write(
        directory.join("expiry.json"),
        serde_json::to_vec_pretty(
            &json!({"seconds":elapsed.as_secs_f64(),"expired_file_exists":false}),
        )?,
    )?;
    let command_id = pending["id"].as_str().ok_or("missing expired command id")?;
    let (status, outcome, _, _) = request(
        &peer,
        "POST",
        &format!("/api/support_terminal/commands/{command_id}/approve"),
        Some(&json!({})),
    )?;
    check(
        status == 400 && outcome["error"] == "command not pending",
        "support expired command approvable",
    )?;
    stop(&peer, &mut [&mut remote, &mut host], first.port)?;
    drop(remote);
    drop(host);

    let second = start(&peer, webhook, 2, "allow_all")?;
    let mut host = owner(port)?;
    let mut controller = guest(&second, true)?;
    let mut viewer = guest(&second, false)?;
    let viewer_denied = directory.join("viewer-denied");
    viewer.send_json(
        &json!({"type":"input","data":format!("touch {}",shell_quote(&viewer_denied))}),
    )?;
    check(
        viewer.event("input_denied")? == json!({"type":"input_denied","reason":"viewer only"}),
        "support viewer input allowed",
    )?;
    viewer.send_json(&json!({"type":"raw","data":"ignored"}))?;
    check(
        viewer.event("input_denied")? == json!({"type":"input_denied","reason":"viewer only"})
            && !viewer_denied.exists(),
        "support viewer raw allowed",
    )?;
    let automatic = directory.join("automatic.txt");
    controller.send_json(&json!({"type":"input","data":format!("printf \"263-auto\" > {}; printf \"AUTO=%s\\n\" 263",shell_quote(&automatic))}))?;
    let approved_event = controller.event("command_approved")?;
    check(
        host.event("command_auto_run")?["id"] == approved_event["id"],
        "support auto-run event changed",
    )?;
    controller.output_until(b"AUTO=263\r\n", 1024 * 1024)?;
    check(
        fs::read_to_string(&automatic)? == "263-auto",
        "support auto-run shell effect missing",
    )?;
    controller.close()?;
    drop(controller);
    loop {
        if viewer.event("control_role")?["granted"] == true {
            break;
        }
    }
    viewer.send_json(&json!({"type":"raw","data":"printf \"HANDOFF=%s\\n\" 263\r"}))?;
    viewer.output_until(b"HANDOFF=263\r\n", 1024 * 1024)?;
    host.close()?;
    drop(host);
    check(
        viewer.event("owner_presence")?["active"] == false,
        "support owner disconnect not observed",
    )?;
    viewer.send_json(&json!({"type":"input","data":format!("touch {}",shell_quote(&denied))}))?;
    check(
        viewer.event("owner_absent")? == json!({"type":"owner_absent"}) && !denied.exists(),
        "support execution after owner disconnect",
    )?;
    stop(&peer, &mut [&mut viewer], second.port)?;
    fs::write(
        directory.join("static.json"),
        serde_json::to_vec_pretty(&static_rows)?,
    )?;
    let results = json!({"owner_without_session_closes":true,"duplicate_start_preserves_session":true,"guest_static_csp_and_api_isolation":true,"five_bad_pin_attempts_close_peer":true,"owner_absent_input_and_raw_denied":true,"typing_sanitizes_controls":true,"approve_each_raw_denied":true,"approval_runs_real_shell_once":true,"rejection_prevents_shell_effect":true,"fifteen_second_expiry_prevents_shell_effect":true,"stop_closes_peers_and_guest_listener":true,"allow_all_controller_viewer_and_handoff":true,"owner_disconnect_revokes_execution":true});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(json!({"scenarios":results,"static":static_rows}))
}

pub fn run_support_frozen(
    config: &SupportFrozenConfig<'_>,
    load_original: impl Fn(&str) -> Result<Value>,
) -> Result<()> {
    fs::create_dir(config.output)?;
    let source = Pin::open(config.source_server, 4 * 1024 * 1024)?;
    let candidate = Pin::open(config.candidate, 256 * 1024 * 1024)?;
    let fixture = Pin::open(config.fixture_binary, 256 * 1024 * 1024)?;
    let child = Pin::open(config.process_child, 256 * 1024 * 1024)?;
    let xterm = Pin::open(
        &config
            .repository
            .join("openpilot/selfdrive/carrot/web/js/vendor/xterm.js"),
        4 * 1024 * 1024,
    )?;
    let original_result = load_original("support/source/result.json")?;
    let original_static = load_original("support/source/static.json")?;
    let original_expiry = load_original("support/source/expiry.json")?;
    let original_cleanup = load_original("support/source/cleanup.json")?;
    let original_http = load_original("support/source/http.json")?;
    let original_wire = load_original("support/source/wire.json")?;
    let original_webhooks = load_original("support/source/webhooks.json")?;
    let original_proof = load_original("support/source/source-proof.json")?;
    if original_proof["sha256"].as_str() != Some(source.digest())
        || original_cleanup["exit"] != 0
        || original_result
            .as_object()
            .is_none_or(|row| row.len() != 13)
        || original_static
            .as_array()
            .is_none_or(|rows| rows.len() != 2)
        || original_http.as_array().is_none_or(|rows| rows.len() != 14)
        || original_wire.as_array().is_none_or(|rows| rows.len() != 98)
        || original_webhooks
            .as_array()
            .is_none_or(|rows| rows.len() != 2)
        || original_expiry["expired_file_exists"] != false
        || !original_expiry["seconds"]
            .as_f64()
            .is_some_and(|seconds| (14.0..=18.0).contains(&seconds))
    {
        return Err("frozen original support observation changed".into());
    }
    let directory = config.output.join("native");
    fs::create_dir(&directory)?;
    let shell = directory.join("fixture-shell");
    fs::write(&shell, "#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n")?;
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700))?;
    let created_shell =
        CreatedProgramGuard::record(&shell, &config.output.join("created-support-shell.jsonl"))?;
    let alias = directory.join("fixture-params/alias");
    fs::create_dir_all(&alias)?;
    symlink("alias", directory.join("fixture-params/d"))?;
    for (name, value) in [
        ("CarName", "owned-support-car"),
        ("DongleId", "owned-support-id"),
        ("HardwareSerial", "owned-support-serial"),
    ] {
        fs::write(alias.join(name), value)?;
    }
    let web = directory.join("openpilot/selfdrive/carrot/web");
    fs::create_dir_all(web.parent().ok_or("missing support web parent")?)?;
    symlink(
        config.repository.join("openpilot/selfdrive/carrot/web"),
        &web,
    )?;
    let webhook = ScriptedHttpPeer::start(vec![Reply::empty(204), Reply::empty(204)])?;
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let argv = [
        OsString::from(config.fixture_binary),
        OsString::from(&directory),
        OsString::from(directory.join("tmux.log")),
        OsString::from(config.process_child),
        port.to_string().into(),
    ];
    let environment = [
        (OsString::from("OPENPILOT_ROOT"), OsString::from(&directory)),
        (
            OsString::from("CARROT_DATA_DIR"),
            OsString::from(directory.join("fixture-data")),
        ),
        (OsString::from("USER"), OsString::from("comma")),
        (OsString::from("SHELL"), OsString::from(&shell)),
        (OsString::from("PS1"), OsString::from("OWNED> ")),
        (OsString::from("PS2"), OsString::from("CONT> ")),
        (
            OsString::from("CARROT_SUPPORT_FAKE_TUNNEL_URL"),
            OsString::from("https://owned-support.invalid"),
        ),
        (
            OsString::from("CARROT_SUPPORT_DISCORD_WEBHOOK_URL"),
            OsString::from(format!("http://127.0.0.1:{}/owned-webhook", webhook.port())),
        ),
        (
            OsString::from("CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE"),
            OsString::from("0"),
        ),
    ];
    let mut server = OwnedServer::spawn(
        &argv,
        config.repository,
        &directory.join("process.log"),
        &environment,
        &[],
        false,
    )?;
    let comparison = (|| -> Result<Value> {
        HttpPeer::new(port, Duration::from_secs(2))
            .wait_ready("/api/support_terminal/status", Duration::from_secs(5))?;
        exercise(port, &directory, &webhook)
    })();
    let stop = server.signal(Signal::TERM);
    let exit = server.finish(Duration::from_secs(8));
    fs::write(
        directory.join("cleanup.json"),
        serde_json::to_vec_pretty(&json!({"stop":stop.is_ok(),"exit":exit.as_ref().ok(),
            "webhook_requests":webhook.requests()?.len()}))?,
    )?;
    let actual = comparison?;
    stop?;
    check(
        exit? == 0 && webhook.pending_replies()? == 0 && webhook.requests()?.len() == 2,
        "native support fixture cleanup failed",
    )?;
    let native_expiry: Value = serde_json::from_slice(&fs::read(directory.join("expiry.json"))?)?;
    check(
        original_result == actual["scenarios"]
            && original_static == actual["static"]
            && native_expiry["expired_file_exists"] == false
            && native_expiry["seconds"]
                .as_f64()
                .is_some_and(|seconds| (14.0..=18.0).contains(&seconds)),
        "frozen original/native support scenario mismatch",
    )?;
    for pin in [&source, &candidate, &fixture, &child, &xterm] {
        pin.verify()?;
    }
    created_shell.finish()?;
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&json!({"pass":true,"comparison":actual,
            "frozen_original_compared":true,"source_sha256":source.digest(),
            "candidate_sha256":candidate.digest(),"fixture_sha256":fixture.digest(),
            "child_sha256":child.digest(),"xterm_asset_sha256":xterm.digest()}))?,
    )?;
    Ok(())
}
