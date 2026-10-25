//! Real original shutdown proxy route; process-local environment only.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    timed_fixtures::Fixture,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[test]
fn original_loopback_proxy_bypass_and_lowercase_precedence() {
    let bytes = include_bytes!("fixtures/timed-tools/proxy-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "d9dd76a85a657b2d6fd24d1f13750f636839db6b38915c33ea15430c04339f38"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 4);
    for case in data["cases"].as_array().unwrap() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(vec![]));
        let captures = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let server = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut reader = BufReader::new(socket.try_clone().unwrap());
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        let path = line.split_whitespace().nth(1).unwrap().to_owned();
                        let mut headers = std::collections::HashMap::new();
                        loop {
                            line.clear();
                            if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                                break;
                            }
                            if let Some((key, value)) = line.split_once(':') {
                                headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
                            }
                        }
                        captures.lock().unwrap().push(json!({"path":path,"agent":headers.get("user-agent"),"encoding":headers.get("accept-encoding")}));
                        let body = br#"{"status":"success","timezone":"Asia/Seoul"}"#;
                        write!(
                            socket,
                            "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n",
                            body.len()
                        )
                        .unwrap();
                        socket.write_all(body).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("proxy fixture: {error}"),
                }
            }
        });
        let directory = tempfile::tempdir().unwrap();
        let fixture = Fixture::new(
            directory.path(),
            "UTC",
            Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
        )
        .unwrap();
        let mut config = fixture.config.clone();
        config["actions"] = json!([{"kind":"internet"}]);
        let mut spec = CommandSpec::new_os(
            vec![env!("CARGO_BIN_EXE_timed-source-reference").into()],
            Duration::from_secs(10),
        );
        fixture.configure(&mut spec);
        let mut environment = spec.effective_env();
        for (key, value) in case["environment"].as_object().unwrap() {
            environment.insert(
                key.into(),
                value.as_str().unwrap().replace("<proxy>", &proxy).into(),
            );
        }
        spec.set_os_env(environment);
        spec.input = Some(format!("{}\n", json!({"config":config,"loop":false})).into_bytes());
        let result = NativeRunner.run(&spec);
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        let captured = result.unwrap();
        assert_eq!(
            captured.code,
            0,
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&captured.stderr)
        );
        let actual: Value = serde_json::from_slice(&captured.stdout).unwrap();
        assert_eq!(actual["values"][0]["ok"], case["value"], "{}", case["name"]);
        assert_eq!(
            json!(*requests.lock().unwrap()),
            case["requests"],
            "{}",
            case["name"]
        );
    }
}
