use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
    sync::mpsc,
};

struct Fixture(PathBuf, bool);
impl Fixture {
    fn new() -> Self {
        let mut bytes = [0; 8];
        getrandom::fill(&mut bytes).unwrap();
        let suffix: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let evidence = std::env::var_os("RECOVERY_SUPPORT_EVIDENCE_DIR").map(PathBuf::from);
        let name = thread::current()
            .name()
            .unwrap_or("fixture")
            .replace(':', "_");
        let path = evidence
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("{name}-{suffix}"));
        fs::create_dir_all(path.join("params/alias")).unwrap();
        symlink("alias", path.join("params/d")).unwrap();
        fs::write(path.join("params/d/CarName"), "owned-car").unwrap();
        if evidence.is_some() {
            println!("OWNED_TRANSPORT_FIXTURE={}", path.display());
        }
        Self(path, evidence.is_some())
    }
    fn invoke(&self, action: &str, webhook: &str, binary: &str) -> std::process::Output {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "support::transport::tests::child_driver",
                "--ignored",
                "--nocapture",
            ])
            .env("SUPPORT_PROBE_ROOT", &self.0)
            .env("SUPPORT_PROBE_ACTION", action)
            .env("CARROT_SUPPORT_DISCORD_WEBHOOK_URL", webhook)
            .env(
                "CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE",
                if action == "notify-disabled" {
                    "1"
                } else {
                    "0"
                },
            )
            .env("CARROT_CLOUDFLARED_BIN", binary)
            .env(
                "CARROT_SUPPORT_FAKE_TUNNEL_URL",
                if action == "fake" {
                    "https://owned.invalid"
                } else {
                    ""
                },
            )
            .output()
            .unwrap();
        fs::write(self.0.join("stdout.txt"), &output.stdout).unwrap();
        fs::write(self.0.join("stderr.txt"), &output.stderr).unwrap();
        fs::write(
            self.0.join("invocation.json"),
            Value::object([
                ("action", Value::text(action)),
                ("webhook", Value::text(webhook)),
                ("binary", Value::text(binary)),
                (
                    "exit_code",
                    Value::integer(output.status.code().unwrap_or(-1)),
                ),
            ])
            .encode()
            .unwrap(),
        )
        .unwrap();
        output
    }
    fn script(&self, body: &str) -> PathBuf {
        let path = self.0.join("cloudflared");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s' $$ > '{}'\n{body}\n",
                self.0.join("pid").display()
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.1 {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn result(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    Value::parse(
        text.lines()
            .find_map(|line| line.strip_prefix("SUPPORT_RESULT="))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn tunnel_drains_and_reaps_after_owned_url_discovery() {
    // Given: an owned executable announces a tunnel and produces more than a pipe buffer.
    let fixture = Fixture::new();
    let binary = fixture.script("printf '%s\\n' 'https://owned-qa.trycloudflare.com/path'\ni=0\nwhile [ $i -lt 1000 ]; do printf '%01000d\\n' 0; i=$((i+1)); done\nprintf 'drained' > drained\nread line");
    // When: the Rust subprocess starts the tunnel and then drops it after draining.
    let output = fixture.invoke("tunnel", "invalid", binary.to_str().unwrap());
    // Then: the exact public origin was captured and the leader is gone after cleanup.
    assert_eq!(
        result(&output),
        Value::text("https://owned-qa.trycloudflare.com")
    );
    assert!(fixture.0.join("drained").is_file());
    let pid = fs::read_to_string(fixture.0.join("pid")).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
}

#[test]
fn failed_tunnel_reaps_when_provider_exits_without_url() {
    // Given: an owned provider emits no matching URL and exits.
    let fixture = Fixture::new();
    let binary = fixture.script("printf '%s\\n' 'no URL'; exit 0");
    // When: tunnel startup fails in a real subprocess.
    let output = fixture.invoke("tunnel-error", "invalid", binary.to_str().unwrap());
    // Then: startup exposes the source error and the exited provider is reaped.
    assert_eq!(
        result(&output),
        Value::text("cloudflared did not provide a trycloudflare URL")
    );
    let pid = fs::read_to_string(fixture.0.join("pid")).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
}

#[test]
fn silent_provider_is_reaped_when_startup_deadline_expires() {
    // Given: an owned provider blocks without ever announcing a URL.
    let fixture = Fixture::new();
    let binary = fixture.script("sleep 120");
    // When: the real tunnel startup reaches its 25-second deadline.
    let started = Instant::now();
    let output = fixture.invoke("tunnel-error", "invalid", binary.to_str().unwrap());
    // Then: startup fails within the deadline cleanup bound and its leader is reaped.
    assert_eq!(
        result(&output),
        Value::text("cloudflared did not provide a trycloudflare URL")
    );
    assert!(started.elapsed() >= Duration::from_secs(25));
    assert!(started.elapsed() < Duration::from_secs(30));
    let pid = fs::read_to_string(fixture.0.join("pid")).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
}

#[test]
fn failed_tunnel_reaps_leader_when_descendant_cleanup_cannot_adopt() {
    // Given: an isolated uninitialized driver under this owned subreaper.
    use rustix::process::{Pid, WaitId, WaitIdOptions, WaitOptions};
    rustix::process::set_child_subreaper(Some(Pid::INIT)).unwrap();
    let fixture = Fixture::new();
    let descendant_file = fixture.0.join("descendant.pid");
    let binary = fixture.script(&format!(
        "sleep 120 &\nprintf '%s' \"$!\" > '{}'\nexit 0",
        descendant_file.display()
    ));
    // When: the provider exits but descendant cleanup cannot adopt its child.
    let output = fixture.invoke(
        "tunnel-error-uninitialized",
        "invalid",
        binary.to_str().unwrap(),
    );
    assert_eq!(
        result(&output),
        Value::text("cloudflared did not provide a trycloudflare URL")
    );
    let leader: i32 = fs::read_to_string(fixture.0.join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    let descendant: i32 = fs::read_to_string(descendant_file)
        .unwrap()
        .parse()
        .unwrap();
    let leader_stat = fs::read_to_string(format!("/proc/{leader}/stat")).ok();
    let descendant_stat = fs::read_to_string(format!("/proc/{descendant}/stat")).unwrap();
    let fields: Vec<_> = descendant_stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    assert_eq!(fields[0], "Z");
    assert_eq!(fields[1].parse::<u32>().unwrap(), std::process::id());
    assert_eq!(fields[3].parse::<i32>().unwrap(), leader);
    for pid in [leader, descendant] {
        let pid = Pid::from_raw(pid).unwrap();
        match rustix::process::waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(_)) => {
                rustix::process::waitpid(Some(pid), WaitOptions::empty()).unwrap();
            }
            Err(rustix::io::Errno::CHILD) => {}
            status => panic!("unexpected owned fixture state: {status:?}"),
        }
    }
    let observation = Value::object([
        ("main_test_pid", Value::integer(std::process::id())),
        ("leader", Value::integer(leader)),
        (
            "leader_reaped_by_tunnel",
            Value::Bool(leader_stat.is_none()),
        ),
        (
            "leader_stat_before_fixture_cleanup",
            leader_stat.as_deref().map_or(Value::Null, Value::text),
        ),
        ("descendant", Value::integer(descendant)),
        (
            "descendant_stat_before_fixture_cleanup",
            Value::text(&descendant_stat),
        ),
    ]);
    fs::write(
        fixture.0.join("ownership.json"),
        observation.encode().unwrap(),
    )
    .unwrap();
    println!(
        "OWNED_TUNNEL_CLEANUP_ERROR={}",
        observation.encode().unwrap()
    );
    // Then: the direct leader was reaped despite the descendant ownership error.
    assert!(
        leader_stat.is_none(),
        "owned leader leaked after descendant cleanup error"
    );
}

#[test]
fn webhook_posts_source_contract_to_owned_recipient() {
    // Given: an owned HTTP recipient and a support session payload.
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/owned-webhook", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let peer = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut input = BufReader::new(socket.try_clone().unwrap());
        let mut first = String::new();
        input.read_line(&mut first).unwrap();
        let mut headers = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                if key.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap();
                }
            }
            headers.push_str(&line.to_lowercase());
        }
        let mut body = vec![0; length];
        input.read_exact(&mut body).unwrap();
        sender
            .send((
                first,
                headers,
                Value::parse(std::str::from_utf8(&body).unwrap()).unwrap(),
            ))
            .unwrap();
        write!(
            socket,
            "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    // When: Rust sends a recovery support notification.
    let output = fixture.invoke("notify", &url, "/unused");
    // Then: response has no body field and real payload disables mentions/embeds.
    assert_eq!(
        result(&output),
        Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(true)),
            ("status", Value::integer(204))
        ])
    );
    let (first, headers, body) = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    fs::write(
        fixture.0.join("request.json"),
        Value::object([
            ("request_line", Value::text(&first)),
            ("headers", Value::text(&headers)),
            ("body", body.clone()),
        ])
        .encode()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(first, "POST /owned-webhook HTTP/1.1\r\n");
    assert!(headers.contains("user-agent: carrotrecovery/2.0\r\n"));
    assert_eq!(
        body.get("allowed_mentions"),
        &Value::object([("parse", Value::Array(vec![]))])
    );
    assert!(body.get("flags").number_eq(4));
    assert!(body
        .get("content")
        .string()
        .unwrap()
        .contains("- Permission: 항상 확인"));
    peer.join().unwrap();
}

#[test]
fn webhook_sends_nothing_when_disabled() {
    // Given: explicit notification disable and an owned listener.
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/owned", listener.local_addr().unwrap());
    // When: disabled support notification is requested.
    let output = fixture.invoke("notify-disabled", &url, "/unused");
    // Then: disabled result is returned without connecting.
    assert_eq!(
        result(&output),
        Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
            ("disabled", Value::Bool(true))
        ])
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
#[ignore = "owned subprocess driver"]
fn child_driver() {
    let root = PathBuf::from(std::env::var_os("SUPPORT_PROBE_ROOT").unwrap());
    let config = Config {
        repository: root.clone(),
        web: root.join("web"),
        launcher: std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("openpilot-process-child"),
        data: root.join("data"),
        params: root.join("params"),
        tmux_log: root.join("tmux.log"),
    };
    let payload = Value::object([
        ("sessionId", Value::text("owned-session")),
        ("createdAt", Value::text("2026-10-12 12:34:56")),
        ("url", Value::text("https://owned.invalid")),
        ("pin", Value::text("123456")),
        ("ttl_minutes", Value::integer(30)),
        ("permissionMode", Value::text("approve_each")),
        ("note", Value::text("owned note")),
        (
            "meta",
            Value::object([
                ("commit", Value::text("deadbeef")),
                ("carName", Value::text("owned-car")),
            ]),
        ),
    ]);
    let action = std::env::var("SUPPORT_PROBE_ACTION").unwrap();
    if action != "tunnel-error-uninitialized" {
        rustix::process::set_child_subreaper(Some(rustix::process::Pid::INIT)).unwrap();
    }
    fs::write(
        root.join("driver-initialization.json"),
        Value::object([
            ("driver_pid", Value::integer(std::process::id())),
            (
                "driver_is_subreaper",
                Value::Bool(rustix::process::child_subreaper().unwrap().is_some()),
            ),
        ])
        .encode()
        .unwrap(),
    )
    .unwrap();
    let output = match action.as_str() {
        "metadata" => metadata(&config).unwrap(),
        "notify" | "notify-disabled" => notify(&config, &payload),
        "tunnel" | "fake" => {
            let tunnel = Tunnel::start(
                &config,
                "http://127.0.0.1:6999",
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            let started = Instant::now();
            while action == "tunnel" && !root.join("drained").is_file() {
                assert!(started.elapsed() < Duration::from_secs(5));
                thread::yield_now();
            }
            Value::text(&tunnel.url)
        }
        "tunnel-error" | "tunnel-error-uninitialized" => match Tunnel::start(
            &config,
            "http://127.0.0.1:6999",
            Arc::new(AtomicBool::new(false)),
        ) {
            Ok(_) => panic!("expected failed provider"),
            Err(error) => Value::text(&error.to_string()),
        },
        _ => panic!("unknown owned action"),
    };
    println!("SUPPORT_RESULT={}", output.encode().unwrap());
}

#[test]
fn webhook_message_matches_actual_recovery_source_with_unicode_and_limits() {
    // Source-only bytes captured from the unchanged _support_message in hosted CI.
    const ORIGINAL: [(&[u8], &[u8]); 2] = [
        (
            include_bytes!(
                "../../../tests/fixtures/recovery-webhook-original/message-0/payload.json"
            ),
            include_bytes!(
                "../../../tests/fixtures/recovery-webhook-original/message-0/source.txt"
            ),
        ),
        (
            include_bytes!(
                "../../../tests/fixtures/recovery-webhook-original/message-1/payload.json"
            ),
            include_bytes!(
                "../../../tests/fixtures/recovery-webhook-original/message-1/source.txt"
            ),
        ),
    ];
    let payloads = [
        Value::object([
            ("createdAt", Value::text("2026-10-12 12:34:56")),
            ("permissionMode", Value::text("allow_all")),
            ("ttl_minutes", Value::text("unlimited")),
            ("note", Value::text(&"가😀".repeat(300))),
        ]),
        Value::object([
            ("createdAt", Value::text("2026-10-12 12:34:56")),
            ("permissionMode", Value::text("other")),
            (
                "meta",
                Value::object([
                    ("carName", Value::text(&"🚙".repeat(2000))),
                    ("commit", Value::text("unknown")),
                ]),
            ),
        ]),
    ];
    for (payload, (original_payload, original_output)) in payloads.into_iter().zip(ORIGINAL) {
        assert_eq!(payload.encode().unwrap().as_bytes(), original_payload);
        let actual = webhook::message(&payload).unwrap();
        assert_eq!(format!("{actual}\n").as_bytes(), original_output);
        assert!(actual.chars().count() <= 1900);
    }
}

#[test]
fn metadata_captures_owned_git_and_params_alias_fallback() {
    // Given: an owned Git repository and Params aliases with DeviceSerial fallback.
    let fixture = Fixture::new();
    assert!(Command::new("git")
        .args(["init", "--quiet", "--initial-branch=owned-support"])
        .arg(&fixture.0)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args([
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "owned transport fixture"
        ])
        .current_dir(&fixture.0)
        .env("GIT_AUTHOR_NAME", "Owned QA")
        .env("GIT_AUTHOR_EMAIL", "qa@example.invalid")
        .env("GIT_COMMITTER_NAME", "Owned QA")
        .env("GIT_COMMITTER_EMAIL", "qa@example.invalid")
        .status()
        .unwrap()
        .success());
    fs::write(
        fixture.0.join("params/d/DeviceSerial"),
        " owned-device-serial \n",
    )
    .unwrap();
    // When: the real subprocess captures recovery support metadata.
    let output = fixture.invoke("metadata", "invalid", "/unused");
    // Then: the repository branch/commit and serial fallback are returned.
    let metadata = result(&output);
    assert_eq!(metadata.get("branch"), &Value::text("owned-support"));
    assert_eq!(metadata.get("serial"), &Value::text("owned-device-serial"));
    assert_eq!(metadata.get("carName"), &Value::text("owned-car"));
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&fixture.0)
        .output()
        .unwrap();
    assert_eq!(
        metadata.get("commit"),
        &Value::text(String::from_utf8(commit.stdout).unwrap().trim())
    );
    println!("OWNED_SUPPORT_METADATA={}", metadata.encode().unwrap());
}

#[test]
fn download_installs_exact_owned_bytes_executable_after_http_transfer() {
    // Given: an owned listener serves a small external-provider executable.
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cloudflared", listener.local_addr().unwrap());
    let bytes = b"#!/bin/sh\nexit 0\n";
    let peer = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut input = BufReader::new(socket.try_clone().unwrap());
        loop {
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        socket.write_all(bytes).unwrap();
    });
    let config = Config {
        repository: fixture.0.clone(),
        web: fixture.0.join("web"),
        launcher: fixture.0.join("unused"),
        data: fixture.0.join("data"),
        params: fixture.0.join("params"),
        tmux_log: fixture.0.join("tmux.log"),
    };
    // When: downloading through the owned HTTP boundary.
    let installed = binary::download(&config, &url).unwrap();
    // Then: exact bytes and executable permissions are atomically installed.
    assert_eq!(fs::read(&installed).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(!config.data.join("bin/cloudflared.tmp").exists());
    peer.join().unwrap();
}
