use openpilot_registration::api_get;
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    time::Duration,
};
#[test]
fn registration_post_query_survives_302_as_get_without_request_body() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", server.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in ["HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", "HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}"] {
            let (mut socket,_)=server.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request=Vec::new();
            while !request.ends_with(b"\r\n\r\n") {let mut byte=[0];socket.read_exact(&mut byte).unwrap();request.extend(byte);}
            requests.push(String::from_utf8(request).unwrap());
            socket.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let response = api_get(
        &host,
        &source,
        "v2/pilotauth/",
        &[("imei", None), ("imei2", Some("a +/한"))],
    )
    .unwrap();
    assert_eq!(response.status, 403);
    let requests = thread.join().unwrap();
    assert!(requests[0].starts_with("POST /v2/pilotauth/?imei2=a+%2B%2F%ED%95%9C HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("GET /next HTTP/1.1\r\n"));
    assert!(requests[0].to_lowercase().contains("content-length: 0\r\n"));
    assert!(!requests[1].to_lowercase().contains("content-length:"));
}
