use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::Path,
    process::Command,
    thread,
    time::Duration,
};
fn child(target: &str, path: &Path, base: &str) -> Command {
    let binary = env!("CARGO_BIN_EXE_openpilot-tombstoned");
    let mut command = if let Ok(runner) = std::env::var("OPENPILOT_CRASH_TEST_RUNNER") {
        let mut parts = runner.split_whitespace();
        let mut command = Command::new(parts.next().unwrap());
        command.args(parts).arg(binary);
        command
    } else {
        Command::new(binary)
    };
    command
        .args(["--process-crash-upload", target])
        .arg(path)
        .env("CARROT_DATA_DIR", path.parent().unwrap().join("data"))
        .env("CARROT_WEB_UPLOAD_URL", base)
        .env("CARROT_TMUX_WEB_UPLOAD_URL", format!("{base}/logs"))
        .env_remove("CARROT_WEB_UPLOAD_TOKEN")
        .env_remove("FAKEUPLOAD");
    command
}
#[test]
fn actual_upload_child_reuses_both_endpoints_and_only_saved_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("1-2-3.json");
    fs::write(
        &path,
        r#"{"event":{},"payload":{"tmux_why":"process_crash","dongle_id":"owned"},"sent":[]}"#,
    )
    .unwrap();
    fs::write(
        path.with_extension("txt"),
        "PROCESS_EXIT owned\n\nsaved crash snapshot 한글",
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let receiver = thread::spawn(move || {
        let mut calls = Vec::new();
        for _ in 0..3 {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(socket);
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            let length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            assert!(length < 1024 * 1024);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let response = if headers.starts_with("POST /api/v1/session ") {
                r#"{"ok":true,"token":"owned-session"}"#
            } else {
                r#"{"ok":true}"#
            };
            write!(reader.get_mut(),"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            calls.push((headers, body));
        }
        calls
    });
    for target in ["dsm", "carrot_logs"] {
        let output = child(target, &path, &base).output().unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let calls = receiver.join().unwrap();
    assert!(calls[0].0.starts_with("POST /api/v1/session "));
    let session: serde_json::Value = serde_json::from_slice(&calls[0].1).unwrap();
    assert_eq!(session["purpose"], "tmux");
    assert!(calls[1].0.starts_with("POST /api/v1/tmux/upload "));
    assert!(calls[1]
        .0
        .to_ascii_lowercase()
        .contains("authorization: bearer owned-session"));
    assert!(calls[2].0.starts_with("POST /logs "));
    assert!(!calls[2].0.to_ascii_lowercase().contains("authorization:"));
    for (_, bytes) in &calls[1..] {
        let body = String::from_utf8_lossy(bytes);
        assert!(body.contains("saved crash snapshot 한글"));
        assert!(body.contains("process_crash"));
        assert!(body.contains("filename=\"tmux.log\""));
        assert!(!body.contains("toggle_values"));
        assert!(!body.contains("files[1]"));
    }
    let output = child("dsm", &path, &base)
        .env("FAKEUPLOAD", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let output = child("invalid", &path, &base).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
}
