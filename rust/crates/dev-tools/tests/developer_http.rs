#![cfg(feature = "developer-ssh-native")]
//! Real private-loopback comparisons; no account, credential or external API access.
use openpilot_dev_tools::developer_ssh::{http::NativeTransport, Get};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    thread,
    time::Duration,
};

#[derive(Debug, PartialEq, Eq)]
struct Request {
    start: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}
fn receive(stream: &TcpStream) -> Request {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let start = line.trim_end().to_owned();
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        assert!(line.len() < 8192 && headers.len() < 100);
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        headers.insert(key.to_ascii_lowercase(), value.trim().to_owned());
    }
    let size = headers
        .get("content-length")
        .map_or(0, |v| v.parse::<usize>().unwrap());
    assert!(size < 8192);
    let mut body = vec![0; size];
    reader.read_exact(&mut body).unwrap();
    // Default HTTP library headers are outside the form/redirect behavior under test.
    headers.retain(|name, _| {
        matches!(
            name.as_str(),
            "content-type" | "authorization" | "cookie" | "user-agent"
        )
    });
    Request {
        start,
        headers,
        body,
    }
}

fn capture(status: u16, native: bool) -> Vec<Request> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut captured = Vec::new();
        while captured.len() < 2 {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "HTTP client did not connect"
                    );
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            captured.push(receive(&stream));
            if captured.len() == 1 {
                write!(stream, "HTTP/1.1 {status} Redirect\r\nLocation: /target\r\nSet-Cookie: test=owned; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{\"success\":true}").unwrap();
            }
        }
        captured
    });
    let url = format!("http://{address}/begin");
    if native {
        let result = NativeTransport
            .post_form(
                &Get {
                    url,
                    authorization: Some("JWT synthetic-test-only".into()),
                    user_agent: "OpenpilotTools",
                    timeout: Some(Duration::from_secs(5)),
                },
                &[("code", "a+b /雪"), ("provider", "h")],
            )
            .unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(result.body, b"{\"success\":true}");
    } else {
        let home = tempfile::tempdir().unwrap();
        let output = Command::new("python3")
            .args([
                "-I", "-c",
                "import requests,sys; s=requests.Session(); s.trust_env=False; s.headers['User-agent']='OpenpilotTools'; s.headers['Authorization']='JWT synthetic-test-only'; r=s.post(sys.argv[1], data={'code':'a+b /雪','provider':'h'}, timeout=5); assert r.status_code==200 and r.content==b'{\"success\":true}'",
                &url,
            ])
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    peer.join().unwrap()
}

#[test]
fn form_post_redirects_match_original_requests_body_cookie_and_auth_behavior() {
    for status in [301, 302, 303, 307, 308] {
        let original = capture(status, false);
        let native = capture(status, true);
        assert_eq!(native, original, "redirect {status}");
        assert_eq!(native[0].start, "POST /begin HTTP/1.1");
        assert_eq!(native[0].body, b"code=a%2Bb+%2F%E9%9B%AA&provider=h");
        if status <= 303 {
            assert_eq!(native[1].start, "GET /target HTTP/1.1");
            assert!(native[1].body.is_empty());
        } else {
            assert_eq!(native[1].start, "POST /target HTTP/1.1");
            assert_eq!(native[1].body, native[0].body);
        }
    }
}
