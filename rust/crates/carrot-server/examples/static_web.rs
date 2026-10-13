use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, header, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{config::Config, static_web::StaticWeb, Value};
use std::{
    convert::Infallible,
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let path = |name| -> Result<PathBuf, openpilot_carrot_navi::Error> {
        Ok(input.get(name).string()?.into())
    };
    let mut config = Config::at(&path("repository")?, &path("data")?, &path("settings")?);
    config.web = path("web")?;
    config.shared_assets = path("shared_assets")?;
    config.training_assets = path("training_assets")?;
    let web = StaticWeb::new(config);
    web.validate()?;
    let bootstrap = input.get("bootstrap").clone();
    let raw_headers = !input.has("raw_headers") || input.get("raw_headers").truth();
    let transport_fixture = input.get("transport_fixture").truth();
    if input.get("precompress").truth() {
        web.start_precompress().await??;
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("fixture server already stopped");
        }
    });
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let web = Arc::clone(&web);
                let bootstrap = bootstrap.clone();
                connections.spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let web = Arc::clone(&web);
                        let bootstrap = bootstrap.clone();
                        async move {
                            let response = if transport_fixture && request.uri().path() == "/__transport_fixture" {
                                transport_response(request).await
                            } else { web.handle(&request, Some(bootstrap)).await };
                            Ok::<_, Infallible>(response)
                        }
                    });
                    let mut builder = hyper::server::conn::http1::Builder::new();
                    if raw_headers { builder.preserve_raw_conditional_headers(true); }
                    if let Err(error) = builder.serve_connection(TokioIo::new(stream), service).await {
                        eprintln!("fixture connection: {error}");
                    }
                });
            }
        }
    }
    connections.abort_all();
    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            if !error.is_cancelled() {
                return Err(error.into());
            }
        }
    }
    input_worker
        .join()
        .map_err(|_| "fixture input thread panicked")?;
    Ok(())
}

async fn transport_response(
    request: Request<Incoming>,
) -> Response<openpilot_carrot_server::http::Body> {
    let names = [
        header::RANGE,
        header::IF_RANGE,
        header::IF_MATCH,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
        header::IF_UNMODIFIED_SINCE,
        header::HeaderName::from_static("x-other"),
    ];
    let raw = request
        .extensions()
        .get::<hyper::ext::RawConditionalHeaders>();
    let values = |original: bool| {
        Value::Object(
            names
                .iter()
                .map(|name| {
                    let value = if original {
                        raw.and_then(|raw| raw.get(name))
                    } else {
                        request.headers().get(name)
                    };
                    (
                        name.as_str().chars().map(u32::from).collect(),
                        value
                            .and_then(|value| value.to_str().ok())
                            .map_or(Value::Null, Value::text),
                    )
                })
                .collect(),
        )
    };
    let normal = values(false);
    let original = values(true);
    let present = raw.is_some();
    let body = match request.into_body().collect().await {
        Ok(body) => body.to_bytes(),
        Err(error) => {
            eprintln!("fixture body: {error}");
            return Response::builder()
                .status(500)
                .body(Full::new(Bytes::new()))
                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())));
        }
    };
    let body_hex = body
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let payload = Value::object([
        ("extension", Value::Bool(present)),
        ("normal", normal),
        ("raw", original),
        ("body_hex", Value::text(&body_hex)),
    ]);
    match payload.encode() {
        Ok(json) => Response::new(Full::new(Bytes::from(json))),
        Err(error) => {
            eprintln!("fixture JSON: {error}");
            Response::new(Full::new(Bytes::new()))
        }
    }
}
