use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use openpilot_webjoystick::handle;

#[tokio::test]
async fn original_routes_serve_without_any_python_source() {
    let response = handle(
        Request::builder()
            .uri("/")
            .body(Full::new(Bytes::new()))
            .unwrap(),
        "http://127.0.0.1:1/stream",
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &include_bytes!("../assets/static/index.html")[..]
    );
    for method in [Method::GET, Method::HEAD] {
        let response = handle(
            Request::builder()
                .method(method.clone())
                .uri("/ping")
                .body(Full::new(Bytes::new()))
                .unwrap(),
            "http://127.0.0.1:1/stream",
        )
        .await;
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            bytes.as_ref(),
            if method == Method::HEAD {
                b"".as_ref()
            } else {
                b"pong".as_ref()
            }
        );
    }
    for path in [
        "/static/main.css",
        "/static/js/webrtc.js",
        "/static/poster.png",
    ] {
        assert_eq!(
            handle(
                Request::builder()
                    .uri(path)
                    .body(Full::new(Bytes::new()))
                    .unwrap(),
                "http://127.0.0.1:1/stream"
            )
            .await
            .status(),
            200
        );
    }
    assert_eq!(
        handle(
            Request::builder()
                .uri("/static/../cert.pem")
                .body(Full::new(Bytes::new()))
                .unwrap(),
            "http://127.0.0.1:1/stream"
        )
        .await
        .status(),
        404
    );
}

#[tokio::test]
async fn offer_preserves_bridge_contract_and_rejects_failed_upstream() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    for (status, answer, content_type, expected) in [
        (
            200,
            "{\"sdp\":\"answer\",\"type\":\"answer\"}",
            "application/json",
            200,
        ),
        (503, "{}", "application/json", 500),
        (200, "invalid", "application/json", 500),
        (200, "{}", "text/plain", 500),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream = format!("http://{}/stream", listener.local_addr().unwrap());
        let peer = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(header) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let text = std::str::from_utf8(&bytes[..header]).unwrap();
                    let length: usize = text
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= header + 4 + length {
                        let sent: serde_json::Value =
                            serde_json::from_slice(&bytes[header + 4..header + 4 + length])
                                .unwrap();
                        assert_eq!(
                            sent,
                            serde_json::json!({"sdp":"offer", "cameras":["driver"], "bridge_services_in":["testJoystick"], "bridge_services_out":["carState"]})
                        );
                        break;
                    }
                }
            }
            stream.write_all(format!("HTTP/1.1 {status} reply\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",answer.len()).as_bytes()).await.unwrap();
        });
        let response = handle(
            Request::builder()
                .method(Method::POST)
                .uri("/offer")
                .body(Full::new(Bytes::from_static(b"{\"sdp\":\"offer\"}")))
                .unwrap(),
            &upstream,
        )
        .await;
        assert_eq!(response.status(), expected);
        peer.await.unwrap();
    }
    for bytes in [b"{}".as_ref(), b"bad".as_ref()] {
        assert_eq!(
            handle(
                Request::builder()
                    .method(Method::POST)
                    .uri("/offer")
                    .body(Full::new(Bytes::copy_from_slice(bytes)))
                    .unwrap(),
                "http://127.0.0.1:1/stream"
            )
            .await
            .status(),
            500
        );
    }
}

#[tokio::test]
async fn https_identity_is_reused_and_shutdown_releases_the_listener() {
    use openpilot_webjoystick::{serve, tls_config};
    let directory = tempfile::tempdir().unwrap();
    let config = tls_config(directory.path(), std::path::Path::new("/usr/bin/openssl")).unwrap();
    let cert_path = directory.path().join("openpilot/tools/bodyteleop/cert.pem");
    let key_path = directory.path().join("openpilot/tools/bodyteleop/key.pem");
    let cert = std::fs::read(&cert_path).unwrap();
    let key = std::fs::read(&key_path).unwrap();
    tls_config(directory.path(), std::path::Path::new("/does-not-exist")).unwrap();
    assert_eq!(std::fs::read(cert_path).unwrap(), cert);
    assert_eq!(std::fs::read(&key_path).unwrap(), key);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, rx) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(serve(
        listener,
        config,
        "http://127.0.0.1:1/stream".into(),
        rx,
    ));
    // Only this owned localhost fixture trusts its generated self-signed identity.
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("https://{address}/ping"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "pong"
    );
    let stalled = tokio::net::TcpStream::connect(address).await.unwrap();
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
    drop(stalled);
    tokio::net::TcpListener::bind(address).await.unwrap();
    std::fs::write(key_path, b"invalid existing private key").unwrap();
    assert!(tls_config(directory.path(), std::path::Path::new("/does-not-exist")).is_err());
}
