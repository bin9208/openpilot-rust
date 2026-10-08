use std::{
    io::{Read, Write},
    net::TcpListener,
};

fn recipient(
    body: Vec<u8>,
    encoding: &'static str,
    length: Option<usize>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("owned listener");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let thread = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().expect("request");
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).expect("header");
                bytes.push(byte[0]);
            }
            let (body, encoding) = if index == 0 {
                (body.as_slice(), encoding)
            } else {
                (b"next".as_slice(), "")
            };
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                if index == 0 {
                    length.unwrap_or(body.len())
                } else {
                    body.len()
                },
                if encoding.is_empty() {
                    String::new()
                } else {
                    format!("Content-Encoding: {encoding}\r\n")
                }
            );
            stream
                .write_all(header.as_bytes())
                .expect("response header");
            stream.write_all(body).expect("response body");
        }
    });
    (endpoint, thread)
}

#[test]
fn raw_and_decoded_readers_diverge_only_in_content_decoding() {
    // Given: actual gzip and Brotli responses from separate owned recipients.
    let source = b"raw mapbox bytes";
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(source).expect("gzip");
    let brotli = crate::static_web::brotli::Brotli::load().expect("native Brotli encoder");
    let bodies = [
        ("gzip", gzip.finish().expect("gzip finish")),
        ("br", brotli.compress(source).expect("Brotli")),
    ];
    // When: each identical response is read by the unchanged and the new API.
    for (encoding, body) in bodies {
        let (endpoint, thread) = recipient(body.clone(), encoding, None);
        let agent = ureq::Agent::config_builder()
            .max_idle_connections(0)
            .build()
            .new_agent();
        let mut decoded = Vec::new();
        let mut decoded_response = agent.get(&endpoint).call().expect("decoded response");
        assert!(!decoded_response.headers().contains_key("content-encoding"));
        assert!(!decoded_response.headers().contains_key("content-length"));
        assert!(decoded_response
            .extensions()
            .get::<ureq::InitialBodyFullyBuffered>()
            .is_some());
        assert_eq!(
            decoded_response
                .extensions()
                .get::<ureq::RawContentEncoding>()
                .map(|encoding| encoding.0.as_bytes()),
            Some(encoding.as_bytes()),
        );
        decoded_response
            .body_mut()
            .as_reader()
            .read_to_end(&mut decoded)
            .expect("decode");
        let mut next = Vec::new();
        let mut next_response = agent.get(&endpoint).call().expect("next response");
        assert!(next_response
            .extensions()
            .get::<ureq::RawContentEncoding>()
            .is_none());
        assert_eq!(
            next_response
                .headers()
                .get("content-length")
                .expect("ordinary length"),
            "4"
        );
        assert!(next_response
            .extensions()
            .get::<ureq::InitialBodyFullyBuffered>()
            .is_some());
        next_response
            .body_mut()
            .as_reader()
            .read_to_end(&mut next)
            .expect("next read");
        thread.join().expect("recipient");
        let (raw_endpoint, raw_thread) = recipient(body.clone(), encoding, None);
        let mut raw = Vec::new();
        let mut raw_response = agent.get(&raw_endpoint).call().expect("raw response");
        assert_eq!(
            raw_response
                .extensions()
                .get::<ureq::RawContentEncoding>()
                .map(|encoding| encoding.0.as_bytes()),
            Some(encoding.as_bytes()),
        );
        raw_response
            .body_mut()
            .as_raw_reader()
            .read_to_end(&mut raw)
            .expect("raw read");
        agent
            .get(&raw_endpoint)
            .call()
            .expect("raw next response")
            .body_mut()
            .as_raw_reader()
            .read_to_end(&mut Vec::new())
            .expect("raw next read");
        raw_thread.join().expect("raw recipient");
        // Then: default decoding still yields source bytes and raw yields wire bytes.
        assert_eq!(decoded, source);
        assert_eq!(raw, body);
        assert_eq!(next, b"next");
    }
}

#[test]
fn initial_buffer_metadata_keeps_late_declared_bytes_and_other_framing_distinct() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("owned listener");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let (release, waiting) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        for index in 0..3 {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).expect("request head");
                request.push(byte[0]);
            }
            let header = match index {
                0 => {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n".as_slice()
                }
                1 => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                    .as_slice(),
                _ => b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".as_slice(),
            };
            stream.write_all(header).expect("response head");
            if index == 0 {
                waiting.recv().expect("release body after header receipt");
            }
            stream
                .write_all(if index == 1 {
                    b"4\r\nnext\r\n0\r\n\r\n"
                } else {
                    b"next"
                })
                .expect("response body");
        }
    });
    let agent = ureq::Agent::config_builder()
        .max_idle_connections(0)
        .build()
        .new_agent();
    for index in 0..3 {
        let mut response = agent.get(&endpoint).call().expect("response");
        let state = response
            .extensions()
            .get::<ureq::InitialBodyFullyBuffered>()
            .map(|state| state.0);
        if index == 0 {
            assert_eq!(state, Some(false));
            release.send(()).expect("release declared body");
        } else {
            assert_eq!(state, None);
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_raw_reader()
            .read_to_end(&mut bytes)
            .expect("framed bytes");
        assert_eq!(bytes, b"next");
    }
    peer.join().expect("recipient");
}

#[test]
fn raw_metadata_preserves_identity_and_unknown_encoding_headers() {
    for encoding in ["identity", "deflate", "unknown"] {
        let (endpoint, thread) = recipient(b"plain bytes".to_vec(), encoding, None);
        let agent = ureq::Agent::config_builder()
            .max_idle_connections(0)
            .build()
            .new_agent();
        let mut response = agent.get(&endpoint).call().expect("ordinary response");
        assert!(response
            .extensions()
            .get::<ureq::RawContentEncoding>()
            .is_none());
        assert_eq!(
            response
                .headers()
                .get("content-encoding")
                .expect("original encoding"),
            encoding
        );
        assert_eq!(
            response
                .headers()
                .get("content-length")
                .expect("original length"),
            "11"
        );
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .read_to_end(&mut body)
            .expect("ordinary bytes");
        assert_eq!(body, b"plain bytes");
        let mut next = agent.get(&endpoint).call().expect("next response");
        assert!(next
            .extensions()
            .get::<ureq::RawContentEncoding>()
            .is_none());
        let mut next_bytes = Vec::new();
        next.body_mut()
            .as_raw_reader()
            .read_to_end(&mut next_bytes)
            .expect("next raw bytes");
        assert_eq!(next_bytes, b"next");
        thread.join().expect("recipient");
    }
}

#[test]
fn partial_raw_read_does_not_poison_the_next_connection() {
    // Given: a response longer than the caller's byte cap and a following request.
    let (endpoint, thread) = recipient(vec![42; 8192], "", None);
    let agent = ureq::Agent::config_builder()
        .max_idle_connections(2)
        .build()
        .new_agent();
    // When: the raw reader consumes a prefix and its response is dropped.
    let mut prefix = Vec::new();
    {
        let mut response = agent.get(&endpoint).call().expect("first response");
        response
            .body_mut()
            .as_raw_reader()
            .take(7)
            .read_to_end(&mut prefix)
            .expect("capped read");
    }
    let mut next = Vec::new();
    agent
        .get(&endpoint)
        .call()
        .expect("next response")
        .body_mut()
        .as_raw_reader()
        .read_to_end(&mut next)
        .expect("next read");
    thread.join().expect("recipient");
    // Then: cap and independent following response retain their complete bytes.
    assert_eq!(prefix, vec![42; 7]);
    assert_eq!(next, b"next");
}
