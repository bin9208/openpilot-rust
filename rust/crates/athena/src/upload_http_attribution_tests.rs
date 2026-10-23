//! Actual upload calls in child test processes expose stderr without global capture hooks.
use super::upload;
use crate::{http_socket, policy::UploadItem};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct TypedFailure(String);
impl<In: ureq::unversioned::transport::Transport> ureq::unversioned::transport::Connector<In>
    for TypedFailure
{
    type Out = In;
    fn connect(
        &self,
        _: &ureq::unversioned::transport::ConnectionDetails<'_>,
        _: Option<In>,
    ) -> Result<Option<In>, ureq::Error> {
        Err(match self.0.as_str() {
            "typed_timeout" => ureq::Error::Timeout(ureq::Timeout::RecvResponse),
            "typed_tls" => ureq::Error::Tls("SENSITIVE_ERROR_TEXT"),
            "typed_reset" => {
                std::io::Error::new(std::io::ErrorKind::ConnectionReset, "SENSITIVE_ERROR_TEXT")
                    .into()
            }
            "typed_wrapped_timeout" => ureq::Error::Io(std::io::Error::other(
                ureq::Error::Timeout(ureq::Timeout::SendBody),
            )),
            _ => unreachable!(),
        })
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "owned peer accept deadline");
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("owned peer accept: {error:?}"),
        }
    }
}

fn read_request(stream: &mut TcpStream) -> bool {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(1) => header.push(byte[0]),
            _ => return false,
        }
        assert!(header.len() < 8192);
    }
    let size = String::from_utf8(header)
        .unwrap()
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(str::to_owned)
        })
        .map_or(0, |size| size.parse::<usize>().unwrap());
    assert!(size <= 1024);
    stream.read_exact(&mut vec![0; size]).is_ok()
}

#[test]
#[ignore = "invoked by failure_receipts_are_sanitized_bounded_and_locate_real_upload_failures"]
fn receipt_child() {
    let mode = std::env::var("ATHENA_ATTRIBUTION_CASE").unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("SENSITIVE_FILE_PATH");
    if mode != "missing" {
        std::fs::write(&path, b"owned upload body").unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer_mode = mode.clone();
    let peer = (!mode.starts_with("typed_") && mode != "missing").then(|| std::thread::spawn(move || {
        let mut stream = accept(&listener);
        if !read_request(&mut stream) {
            assert_eq!(peer_mode, "callback");
            return;
        }
        match peer_mode.as_str() {
            "request_eof" => {}
            "response_body" => stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nx").unwrap(),
            "redirect_request" => {
                stream.write_all(b"HTTP/1.1 303 See Other\r\nLocation: /SENSITIVE_REDIRECT\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                drop(stream);
                let mut redirected = accept(&listener);
                assert!(read_request(&mut redirected));
            }
            "redirect_parse" => stream.write_all(b"HTTP/1.1 303 See Other\r\nLocation: http://[invalid\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap(),
            "success" => stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap(),
            _ => panic!("unexpected owned peer case"),
        }
    }));
    let item = UploadItem {
        path: path.to_str().unwrap().to_owned(),
        url: format!("http://127.0.0.1:{port}/SENSITIVE_URL?token=SENSITIVE_QUERY"),
        headers: serde_json::Map::from_iter([("Authorization".into(), "SENSITIVE_HEADER".into())]),
        created_at: 0,
        id: Some("SENSITIVE_ITEM_ID".into()),
        retry_count: 0,
        current: false,
        progress: 0.,
        allow_cellular: true,
        priority: 0,
    };
    let agent = if mode.starts_with("typed_") {
        use ureq::unversioned::transport::Connector;
        ureq::Agent::with_parts(
            ureq::Agent::config_builder().build(),
            ().chain(TypedFailure(mode.clone())),
            ureq::unversioned::resolver::DefaultResolver::default(),
        )
    } else {
        http_socket::agent()
    };
    let result = upload(&agent, &item, |_, _| {
        if mode == "callback" {
            Err(std::io::Error::other("SENSITIVE_ERROR_TEXT"))
        } else {
            Ok(())
        }
    });
    assert_eq!(result.is_ok(), mode == "success");
    if let Some(peer) = peer {
        peer.join().unwrap();
    }
}

#[test]
fn failure_receipts_are_sanitized_bounded_and_locate_real_upload_failures() {
    for (case, phase, category, detail, hop) in [
        ("missing", "prepare_body", "io", "not_found", 0),
        ("callback", "progress_callback", "io", "other", 0),
        ("request_eof", "request", "io", "unexpected_eof", 0),
        ("response_body", "response_body", "io", "unexpected_eof", 0),
        ("redirect_request", "request", "io", "unexpected_eof", 1),
        ("redirect_parse", "redirect", "url", "none", 0),
        ("typed_timeout", "request", "timeout", "recv_response", 0),
        ("typed_tls", "request", "tls", "none", 0),
        ("typed_reset", "request", "io", "connection_reset", 0),
        (
            "typed_wrapped_timeout",
            "request",
            "timeout",
            "send_body",
            0,
        ),
        ("success", "", "", "", 0),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "upload_http::attribution_tests::receipt_child",
                "--ignored",
                "--nocapture",
            ])
            .env("ATHENA_ATTRIBUTION_CASE", case)
            .env_remove("HTTP_PROXY")
            .env_remove("http_proxy")
            .env_remove("HTTPS_PROXY")
            .env_remove("https_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "owned upload child failed in {case}"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            !stderr.contains("SENSITIVE_"),
            "receipt exposed controlled private data"
        );
        let receipts: Vec<_> = stderr
            .lines()
            .filter_map(|line| line.strip_prefix("ATHENA_UPLOAD_FAILURE "))
            .collect();
        if case == "success" {
            assert!(receipts.is_empty());
            continue;
        }
        assert_eq!(
            receipts.len(),
            1,
            "{case}: exactly one receipt per failed upload call"
        );
        assert!(receipts[0].len() + "ATHENA_UPLOAD_FAILURE ".len() < 512);
        let receipt: serde_json::Value = serde_json::from_str(receipts[0]).unwrap();
        assert_eq!(receipt["phase"], phase, "{case}");
        assert_eq!(receipt["category"], category, "{case}");
        assert_eq!(receipt["detail"], detail, "{case}");
        assert_eq!(receipt["redirect_index"], hop, "{case}");
        let observed_status = match case {
            "response_body" => 200,
            "redirect_parse" => 303,
            _ => 0,
        };
        assert_eq!(receipt["response_status"], observed_status, "{case}");
        assert!(receipt["attempt"].as_u64().unwrap() > 0);
        assert!(receipt["pid"].as_u64().unwrap() > 0);
        assert!(receipt["start_mono_ns"].as_i64().unwrap() > 0);
        assert!(
            receipt["end_mono_ns"].as_i64().unwrap() >= receipt["start_mono_ns"].as_i64().unwrap()
        );
        assert!(receipt["start_wall_ns"].as_u64().unwrap() > 0);
        assert!(receipt["end_wall_ns"].as_u64().unwrap() > 0);
        let mut keys: Vec<_> = receipt
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "attempt",
                "category",
                "detail",
                "end_mono_ns",
                "end_wall_ns",
                "phase",
                "pid",
                "redirect_index",
                "response_status",
                "start_mono_ns",
                "start_wall_ns"
            ]
        );
    }
}
