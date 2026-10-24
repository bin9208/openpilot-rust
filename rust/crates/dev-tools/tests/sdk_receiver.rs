//! Actual original Python receiver wires, replayed through the native loopback server.
use openpilot_dev_tools::sdk_receiver::{decode_envelope, Receiver};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, Instant},
};
fn original() -> Value {
    let bytes = include_bytes!("fixtures/sdk-receiver/original-http.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "910129d4cbb8786a7eaafde33bbff973c296b9f035057cf9d0e0919dca452a5c"
    );
    serde_json::from_slice(bytes).unwrap()
}
fn wire(row: &Value) -> Vec<u8> {
    row["wire_hex"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn send(server: &Receiver, row: &Value) -> String {
    let mut client = TcpStream::connect(server.address()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let bytes = wire(row);
    write!(client, "POST /api/1/envelope/ HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nContent-Type: application/x-sentry-envelope\r\nX-Sentry-Auth: fixture-auth\r\n",server.address(),bytes.len()).unwrap();
    if let Some(encoding) = row["content_encoding"].as_str() {
        write!(client, "Content-Encoding: {encoding}\r\n").unwrap();
    }
    client.write_all(b"\r\n").unwrap();
    client.write_all(&bytes).unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    response
}
#[test]
fn actual_original_identity_gzip_brotli_wires_keep_event_raw_and_auth() {
    let mut receiver = Receiver::bind(Duration::from_secs(3)).unwrap();
    for row in original()["http_cases"].as_array().unwrap() {
        let response = send(&receiver, row);
        assert!(response.starts_with("HTTP/1.0 200 OK\r\n"));
        assert!(response.ends_with("\r\n\r\n{}"));
        let captured = receiver.receive(Duration::from_secs(3)).unwrap();
        assert_eq!(captured.raw, row["raw"].as_str().unwrap());
        assert_eq!(captured.wire, wire(row));
        assert_eq!(
            serde_json::from_str::<Value>(&captured.event.to_json().unwrap()).unwrap(),
            row["event"]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&captured.header.to_json().unwrap()).unwrap(),
            row["header"]
        );
        assert_eq!(captured.path, "/api/1/envelope/");
        assert_eq!(captured.auth.as_deref(), Some("fixture-auth"));
        assert_eq!(
            captured.content_encoding.as_deref(),
            row["content_encoding"].as_str()
        );
    }
    receiver.close().unwrap();
}
#[test]
fn decoder_rejects_unsupported_and_malformed_encodings() {
    for row in original()["http_cases"].as_array().unwrap() {
        assert_eq!(
            decode_envelope(&wire(row), row["content_encoding"].as_str()).unwrap(),
            row["raw"].as_str().unwrap().as_bytes()
        );
    }
    assert!(decode_envelope(b"{}", Some("unsupported-fixture")).is_err());
    for encoding in ["gzip", "br"] {
        assert!(decode_envelope(b"not compressed", Some(encoding)).is_err());
    }
    assert_eq!(
        decode_envelope(b"literal", Some("identity")).unwrap(),
        b"literal"
    );
}
#[test]
fn partial_clients_do_not_block_other_requests_or_receiver_shutdown() {
    let mut receiver = Receiver::bind(Duration::from_secs(20)).unwrap();
    let mut partial_header = TcpStream::connect(receiver.address()).unwrap();
    partial_header
        .write_all(b"POST /api/1/envelope/ HTTP/1.1\r\n")
        .unwrap();
    let mut partial_body = TcpStream::connect(receiver.address()).unwrap();
    partial_body
        .write_all(b"POST /api/1/envelope/ HTTP/1.1\r\nContent-Length: 100000\r\n\r\nx")
        .unwrap();
    assert!(send(&receiver, &original()["http_cases"][0]).contains("200 OK"));
    receiver.receive(Duration::from_secs(3)).unwrap();
    let address = receiver.address();
    let start = Instant::now();
    receiver.close().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(TcpStream::connect(address).is_err());
    for mut stream in [partial_header, partial_body] {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {}
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("owned accepted socket still alive: {other:?}"),
        }
        let _ = stream.shutdown(Shutdown::Both);
    }
    receiver.close().unwrap();
}
#[test]
fn accepted_client_deadline_is_total_and_recovers_for_later_event() {
    let mut receiver = Receiver::bind(Duration::from_millis(200)).unwrap();
    let mut client = TcpStream::connect(receiver.address()).unwrap();
    client
        .write_all(b"POST /api/1/envelope/ HTTP/1.1\r\n")
        .unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..100 {
            if client.write_all(b"X").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let start = Instant::now();
    let error = receiver.receive(Duration::from_secs(2)).unwrap_err();
    assert!(error.to_string().contains("deadline"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(send(&receiver, &original()["http_cases"][1]).contains("200 OK"));
    receiver.receive(Duration::from_secs(2)).unwrap();
    receiver.close().unwrap();
    writer.join().unwrap();
}
#[test]
fn command_runs_real_http_without_python_and_keeps_original_report_fields() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("receiver.json");
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-sdk-receiver").into(),
            output.as_os_str().to_owned(),
        ],
        Duration::from_secs(15),
    );
    spec.env.insert("PATH".into(), "/nonexistent".into());
    spec.timeout = Duration::from_secs(15);
    let actual = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        actual.code,
        0,
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert_eq!(
        actual.stdout,
        b"PASS: 3 actual HTTP envelope encodings; unsupported encoding rejected\n"
    );
    let report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["result"], "PASS");
    assert_eq!(report["unsupported_rejected"], true);
    assert_eq!(report["http_cases"].as_array().unwrap().len(), 3);
    for (actual, expected) in report["http_cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(original()["http_cases"].as_array().unwrap())
    {
        for field in [
            "event",
            "raw",
            "path",
            "header",
            "auth",
            "wire_hex",
            "content_encoding",
        ] {
            assert_eq!(actual[field], expected[field], "{field}");
        }
        for (key, value) in expected["headers"].as_object().unwrap() {
            if key != "Host" {
                assert_eq!(&actual["headers"][key], value);
            }
        }
        assert!(actual["headers"]["Host"]
            .as_str()
            .unwrap()
            .starts_with("127.0.0.1:"));
    }
}
#[test]
fn original_delivers_decoded_event_even_when_peer_resets_before_acknowledgement() {
    let captured = include_bytes!("fixtures/sdk-receiver/original-response-error.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(captured)),
        "650ecaf27e0ac0a2ff4b87a90acc379a6154a0e9a7f3fff864a445448e034ed0"
    );
    let original: Value = serde_json::from_slice(captured).unwrap();
    assert_eq!(original["cases"].as_array().unwrap().len(), 3);
    for case in original["cases"].as_array().unwrap() {
        let mut receiver = Receiver::bind(Duration::from_secs(2)).unwrap();
        assert_eq!(case["event_queued_despite_closed_response_peer"], true);
        let row = &case["captured"];
        let bytes = wire(row);
        let mut client = TcpStream::connect(receiver.address()).unwrap();
        rustix::net::sockopt::set_socket_linger(&client, Some(Duration::ZERO)).unwrap();
        let mut request = format!(
            "POST /api/1/envelope/ HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            bytes.len()
        )
        .into_bytes();
        request.extend(&bytes);
        client.write_all(&request).unwrap();
        drop(client);
        let event = receiver.receive(Duration::from_secs(3)).unwrap();
        assert_eq!(event.raw, row["raw"].as_str().unwrap());
        receiver.close().unwrap();
    }
}
#[test]
fn help_exits_without_running_http_or_creating_output_file() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    for flag in ["--help", "-h"] {
        let temp = tempfile::tempdir().unwrap();
        let mut spec = CommandSpec::new(
            vec![env!("CARGO_BIN_EXE_check-sdk-receiver").into(), flag.into()],
            Duration::from_secs(5),
        );
        spec.cwd = Some(temp.path().to_owned());
        spec.env.insert("PATH".into(), "/nonexistent".into());
        let result = NativeRunner.run(&spec).unwrap();
        assert_eq!(result.code, 0);
        assert!(String::from_utf8_lossy(&result.stdout).starts_with("usage:"));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
#[test]
fn failed_event_parse_never_publishes_a_capture_and_next_client_recovers() {
    let mut receiver = Receiver::bind(Duration::from_secs(2)).unwrap();
    let mut client = TcpStream::connect(receiver.address()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .write_all(b"POST /api/1/envelope/ HTTP/1.1\r\nContent-Length: 8\r\n\r\n{}\n{}\n{}")
        .unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    assert!(response.is_empty());
    assert!(receiver
        .receive(Duration::from_secs(2))
        .unwrap_err()
        .to_string()
        .contains("not an event"));
    receiver.require_empty().unwrap();
    assert!(send(&receiver, &original()["http_cases"][2]).contains("200 OK"));
    receiver.receive(Duration::from_secs(2)).unwrap();
    receiver.require_empty().unwrap();
    receiver.close().unwrap();
}
