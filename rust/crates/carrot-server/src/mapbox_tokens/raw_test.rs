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
        agent
            .get(&endpoint)
            .call()
            .expect("decoded response")
            .body_mut()
            .as_reader()
            .read_to_end(&mut decoded)
            .expect("decode");
        let mut next = Vec::new();
        agent
            .get(&endpoint)
            .call()
            .expect("next response")
            .body_mut()
            .as_reader()
            .read_to_end(&mut next)
            .expect("next read");
        thread.join().expect("recipient");
        let (raw_endpoint, raw_thread) = recipient(body.clone(), encoding, None);
        let mut raw = Vec::new();
        agent
            .get(&raw_endpoint)
            .call()
            .expect("raw response")
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
