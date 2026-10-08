use crate::Value;
use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use std::{convert::Infallible, error::Error, path::Path, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

const DATA: &[u8] = &[b'Z'; 1024 * 1024];

async fn read_response(reader: &mut BufReader<TcpStream>) -> Result<Vec<String>, Box<dyn Error>> {
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).await?, 0);
        if line == "\r\n" {
            break;
        }
        headers.push(line.trim_end().to_owned());
    }
    assert_eq!(headers[0], "HTTP/1.1 200 OK");
    assert!(headers
        .iter()
        .any(|line| line.eq_ignore_ascii_case("content-length: 1048576")));
    assert!(!headers
        .iter()
        .any(|line| line.to_ascii_lowercase().starts_with("connection:")));
    let mut body = vec![0; DATA.len()];
    reader.read_exact(&mut body).await?;
    assert_eq!(body, DATA);
    Ok(headers)
}

async fn control(
    name: &str,
    enabled: bool,
    marked: bool,
    continue_calls: usize,
) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await?;
        let service = service_fn(move |request: Request<Incoming>| async move {
            if continue_calls > 0 {
                let signal = request
                    .extensions()
                    .get::<hyper::ext::ContinueSignal>()
                    .expect("enabled signal")
                    .clone();
                for _ in 0..continue_calls {
                    signal.send().expect("signal sender");
                }
                drop(request);
                drop(signal);
            }
            let mut response = Response::new(Full::new(Bytes::from_static(DATA)));
            if marked {
                response
                    .extensions_mut()
                    .insert(hyper::ext::CloseAfterResponse);
            }
            Ok::<_, Infallible>(response)
        });
        let mut builder = hyper::server::conn::http1::Builder::new();
        if enabled {
            builder.close_after_response(true);
        }
        if continue_calls > 0 {
            builder.application_continue(true);
        }
        builder
            .serve_connection(TokioIo::new(socket), service)
            .await?;
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });
    let mut client = BufReader::new(TcpStream::connect(("127.0.0.1", port)).await?);
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    client.get_mut().write_all(request).await?;
    if continue_calls > 0 {
        let mut informational = [0; 25];
        client.read_exact(&mut informational).await?;
        assert_eq!(&informational, b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let first = read_response(&mut client).await?;
    let closes = enabled && marked;
    let mut second = None;
    if closes {
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), client.read(&mut byte)).await??,
            0
        );
    } else {
        client.get_mut().write_all(request).await?;
        second = Some(read_response(&mut client).await?);
    }
    drop(client);
    assert!(tokio::time::timeout(Duration::from_secs(1), server)
        .await??
        .is_ok());
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.omo/evidence/carrot-server-225-resume/request-decoding/hyper-tcp");
    std::fs::create_dir_all(&directory)?;
    let headers =
        |rows: Vec<String>| Value::Array(rows.iter().map(|row| Value::text(row)).collect());
    let receipt = Value::object([
        ("scenario", Value::text(name)),
        ("real_tcp", Value::Bool(true)),
        ("option_called", Value::Bool(enabled)),
        ("marked", Value::Bool(marked)),
        ("continue_calls", Value::integer(continue_calls)),
        (
            "informational_responses",
            Value::integer(usize::from(continue_calls > 0)),
        ),
        ("response_body_bytes", Value::integer(DATA.len())),
        (
            "body_sha256",
            Value::text(&format!("{:x}", Sha256::digest(DATA))),
        ),
        ("first_headers", headers(first)),
        ("second_headers", second.map(headers).unwrap_or(Value::Null)),
        ("eof_after_complete_body", Value::Bool(closes)),
        ("passed", Value::Bool(true)),
    ]);
    std::fs::write(directory.join(format!("{name}.json")), receipt.encode()?)?;
    Ok(())
}

#[tokio::test]
async fn default_ignores_marker() -> Result<(), Box<dyn Error>> {
    control("default-ignores-marker", false, true, 0).await
}

#[tokio::test]
async fn enabled_unmarked_keepalive() -> Result<(), Box<dyn Error>> {
    control("enabled-unmarked-keepalive", true, false, 0).await
}

#[tokio::test]
async fn enabled_marker_closes_after_full_response() -> Result<(), Box<dyn Error>> {
    control("enabled-marker-full-eof", true, true, 0).await
}

#[tokio::test]
async fn continue_survives_request_drop_and_repeated_calls() -> Result<(), Box<dyn Error>> {
    control("continue-drop-repeated-once", true, true, 2).await
}
