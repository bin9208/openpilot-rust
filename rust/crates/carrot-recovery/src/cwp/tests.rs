use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    os::unix::fs::symlink,
    process::{Command, Stdio},
    sync::mpsc,
    time::Instant,
};

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = std::env::temp_dir().join(format!("recovery-cwp-{suffix}"));
        fs::create_dir_all(path.join("params/alias")).unwrap();
        symlink("alias", path.join("params/d")).unwrap();
        fs::write(path.join("ip"), "192.168.42.9").unwrap();
        fs::write(path.join("params/d/DongleId"), "owned-cwp-device").unwrap();
        Self { path }
    }

    fn param(&self, key: &str, value: &str) {
        fs::write(self.path.join("params/d").join(key), value).unwrap();
    }

    fn invoke(&self, endpoint: &str, action: &str) -> std::process::Output {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cwp::tests::child_driver",
                "--ignored",
                "--nocapture",
            ])
            .env("CWP_PROBE_ROOT", &self.path)
            .env("CWP_PROBE_ACTION", action)
            .env("CWEB_PUSH_REPORT_URL", endpoint)
            .env("CWP_REPORT_TOKEN", " owned-fixture-token ")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

#[derive(Debug)]
struct Request {
    path: String,
    headers: String,
    body: Value,
}

fn peer(responses: Vec<(u16, &str)>) -> (String, mpsc::Receiver<Request>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/report", listener.local_addr().unwrap());
    let responses: Vec<_> = responses
        .into_iter()
        .map(|(status, body)| (status, body.to_owned()))
        .collect();
    let (sender, receiver) = mpsc::channel();
    let task = thread::spawn(move || {
        for (status, body) in responses {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut input = BufReader::new(socket.try_clone().unwrap());
            let mut first = String::new();
            input.read_line(&mut first).unwrap();
            let path = first.split_whitespace().nth(1).unwrap().to_owned();
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
            let mut bytes = vec![0; length];
            input.read_exact(&mut bytes).unwrap();
            sender
                .send(Request {
                    path,
                    headers,
                    body: Value::parse(std::str::from_utf8(&bytes).unwrap()).unwrap(),
                })
                .unwrap();
            let reason = if status == 200 { "OK" } else { "Unavailable" };
            let mut output = socket;
            write!(output, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, receiver, task)
}

fn result(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let encoded = text
        .lines()
        .find_map(|line| line.strip_prefix("CWP_RESULT="))
        .unwrap();
    Value::parse(encoded).unwrap()
}

#[test]
fn status_clears_enable_alias_when_registration_is_lost() {
    // Given: persistent enable through the real Params d symlink and a loopback service.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "1");
    let (endpoint, requests, task) = peer(vec![(200, r#"{"ok":true,"registered":false}"#)]);
    // When: the standalone process checks registration.
    let output = fixture.invoke(&endpoint, "status");
    // Then: its JSON and persistent alias both disable recovery boot.
    assert_eq!(
        result(&output),
        Value::object([
            ("ok", Value::Bool(true)),
            ("enabled", Value::Bool(false)),
            ("registered", Value::Bool(false)),
            ("state", Value::text("unregistered")),
        ])
    );
    assert_eq!(
        fs::read(fixture.path.join("params/alias/CwebPushRecoveryBoot")).unwrap(),
        b"0"
    );
    assert!(fixture.path.join("params/d").is_symlink());
    let request = requests.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(request.path, "/recovery/status");
    assert_eq!(
        request.body.get("deviceId"),
        &Value::text("owned-cwp-device")
    );
    assert!(request
        .headers
        .contains("user-agent: carrotrecovery/2.0\r\n"));
    assert!(request
        .headers
        .contains("authorization: bearer owned-fixture-token\r\n"));
    task.join().unwrap();
}

#[test]
fn enabling_is_rejected_when_unregistered() {
    // Given: a registered flag is absent and Params are available.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "0");
    let (endpoint, _requests, task) = peer(vec![(200, r#"{"ok":true,"registered":false}"#)]);
    // When: enabling is requested in a real process.
    let output = fixture.invoke(&endpoint, "enable");
    // Then: registration prevents a persistent enable.
    let status = result(&output);
    assert_eq!(status.get("ok"), &Value::Bool(false));
    assert_eq!(status.get("error"), &Value::text("Not registered"));
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"0"
    );
    task.join().unwrap();
}

#[test]
fn enabling_persists_when_registered_with_serial_fallback() {
    // Given: DongleId is a sentinel and HardwareSerial is meaningful.
    let fixture = Fixture::new();
    fixture.param("DongleId", " UNREGISTEREDDEVICE \n");
    fixture.param("HardwareSerial", " owned-serial \r\n");
    let (endpoint, requests, task) = peer(vec![(200, r#"{"ok":true,"registered":"yes"}"#)]);
    // When: registered recovery boot is enabled.
    let output = fixture.invoke(&endpoint, "enable");
    // Then: Python truth semantics and fallback identity reach the real service.
    assert_eq!(result(&output).get("enabled"), &Value::Bool(true));
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"1"
    );
    assert_eq!(
        requests
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .body
            .get("deviceId"),
        &Value::text("owned-serial")
    );
    task.join().unwrap();
}

#[test]
fn disabling_persists_when_service_is_unavailable() {
    // Given: recovery boot was enabled but the loopback server rejects status.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "1");
    let (endpoint, _requests, task) = peer(vec![(503, "owned unavailable")]);
    // When: disabling is requested.
    let output = fixture.invoke(&endpoint, "disable");
    // Then: disable succeeds while preserving service error and unknown registration.
    let status = result(&output);
    assert_eq!(status.get("ok"), &Value::Bool(true));
    assert_eq!(status.get("registered"), &Value::Null);
    assert_eq!(status.get("error"), &Value::text("owned unavailable"));
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"0"
    );
    task.join().unwrap();
}

#[test]
fn failed_status_preserves_enable_when_response_is_invalid() {
    // Given: a enabled recovery registration and a non-object response.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "1");
    let (endpoint, _requests, task) = peer(vec![(200, "[true]")]);
    // When: checking status.
    let output = fixture.invoke(&endpoint, "status");
    // Then: invalid response remains unavailable without clearing persistent enable.
    let status = result(&output);
    assert_eq!(status.get("enabled"), &Value::Bool(true));
    assert_eq!(status.get("state"), &Value::text("unavailable"));
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"1"
    );
    task.join().unwrap();
}

#[test]
fn boot_retries_and_clears_enable_when_registration_is_lost() {
    // Given: the same private IP in an owned file and a transient loopback failure.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "1");
    let (endpoint, requests, task) = peer(vec![
        (503, "try again"),
        (200, r#"{"ok":true,"registered":false,"pushed":"2"}"#),
    ]);
    let started = Instant::now();
    // When: a standalone process performs reboot startup notification.
    let output = fixture.invoke(&endpoint, "boot");
    // Then: stabilization and retry produce two payloads and persist unregistration.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() >= Duration::from_secs(7));
    for _ in 0..2 {
        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(request.path, "/recovery/boot");
        assert_eq!(request.body.get("ip"), &Value::text("192.168.42.9"));
        assert!(request.body.get("port").number_eq(6999));
    }
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"0"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("registered=False pushed=2"));
    task.join().unwrap();
}

#[test]
fn boot_stop_joins_without_reporting_when_ip_is_missing() {
    // Given: persistent enable and an empty owned address file.
    let fixture = Fixture::new();
    fixture.param("CwebPushRecoveryBoot", "1");
    fs::write(fixture.path.join("ip"), "").unwrap();
    // When: a real process starts and cancels the boot worker.
    let started = Instant::now();
    let output = fixture.invoke("http://127.0.0.1:9/report", "boot-stop");
    // Then: the worker joins immediately and retains its gate.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        fs::read(fixture.path.join("params/d/CwebPushRecoveryBoot")).unwrap(),
        b"1"
    );
}

#[test]
fn production_boot_sends_nothing_when_persistent_enable_is_absent() {
    // Given: a real Params directory without a persistent enable and owned listener.
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/report", listener.local_addr().unwrap());
    // When: the production boot entry runs in an owned process.
    let output = fixture.invoke(&endpoint, "boot-disabled");
    // Then: production startup exits without any report or address discovery wait.
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
#[ignore = "owned subprocess driver called only with explicit fixture configuration"]
fn child_driver() {
    let root = PathBuf::from(std::env::var_os("CWP_PROBE_ROOT").unwrap());
    let config = Config {
        repository: root.clone(),
        web: root.join("web"),
        launcher: root.join("launcher"),
        data: root.join("data"),
        params: root.join("params"),
        tmux_log: root.join("tmux.log"),
    };
    let action = std::env::var("CWP_PROBE_ACTION").unwrap();
    let status = match action.as_str() {
        "status" => status(&config).unwrap(),
        "enable" => set_enabled(&config, true).unwrap(),
        "disable" => set_enabled(&config, false).unwrap(),
        "boot" => {
            let mut worker =
                BootWorker::start_with_ip_fixture(&config, 6999, &root.join("ip")).unwrap();
            worker.join().unwrap();
            Value::Bool(true)
        }
        "boot-stop" => {
            let mut worker =
                BootWorker::start_with_ip_fixture(&config, 6999, &root.join("ip")).unwrap();
            worker.stop().unwrap();
            Value::Bool(true)
        }
        "boot-disabled" => {
            let mut worker = BootWorker::start(&config, 6999).unwrap();
            worker.join().unwrap();
            Value::Bool(true)
        }
        _ => panic!("unknown owned action"),
    };
    println!("CWP_RESULT={}", status.encode().unwrap());
}
