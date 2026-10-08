use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    param_changes::{History, Paths},
    param_restore::Restore,
    params::Backend,
    params_multipart::{self, FirstFile},
    settings::Catalog,
    Error, Value,
};
use std::{
    convert::Infallible,
    io::{self, BufRead},
    path::PathBuf,
    sync::{Arc, Mutex},
};

struct Service {
    backend: Mutex<Backend>,
    catalog: Catalog,
    history: History,
}

fn response(status: StatusCode, value: Value) -> Result<Response<Full<Bytes>>, Error> {
    let bytes = value.encode()?;
    let mut response = Response::new(Full::new(Bytes::from(bytes.clone())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response.headers_mut().insert(
        hyper::header::CONTENT_LENGTH,
        hyper::header::HeaderValue::from_str(&bytes.len().to_string())
            .map_err(|error| Error::Source(error.to_string()))?,
    );
    Ok(response)
}

async fn handle(
    request: Request<Incoming>,
    service: Arc<Service>,
) -> Result<Response<Full<Bytes>>, Error> {
    let (status, value) = match params_multipart::first_file(request).await {
        Ok(FirstFile::MissingFileField) => (
            StatusCode::BAD_REQUEST,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text("missing file field")),
            ]),
        ),
        Ok(FirstFile::Data(bytes)) => {
            let parsed = Value::parse(&String::from_utf8_lossy(&bytes));
            match parsed {
                Ok(value @ Value::Object(_)) => {
                    let catalog = service.catalog.with_gap_limits(3)?;
                    let mut backend = service
                        .backend
                        .lock()
                        .map_err(|error| Error::Source(error.to_string()))?;
                    let restored = Restore::new(&mut backend, &catalog, &service.history)
                        .restore_values(&value, &Value::text("restore"));
                    match restored {
                        Ok(result) => (
                            StatusCode::OK,
                            Value::object([("ok", Value::Bool(true)), ("result", result)]),
                        ),
                        Err(error) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Value::object([
                                ("ok", Value::Bool(false)),
                                ("error", Value::text(&error.to_string())),
                            ]),
                        ),
                    }
                }
                Ok(_) => (
                    StatusCode::BAD_REQUEST,
                    Value::object([
                        ("ok", Value::Bool(false)),
                        ("error", Value::text("bad json format (must be object)")),
                    ]),
                ),
                Err(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Value::object([
                        ("ok", Value::Bool(false)),
                        ("error", Value::text(&error.to_string())),
                    ]),
                ),
            }
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ),
    };
    response(status, value)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root = PathBuf::from(input.get("root").string()?);
    let state = PathBuf::from(input.get("state").string()?);
    let service = Arc::new(Service {
        backend: Mutex::new(Backend::native(
            openpilot_params::Params::for_runtime_at(&root)?,
            state.clone(),
        )),
        catalog: Catalog::from_data(input.get("catalog").clone())?,
        history: History::new(Paths {
            log: state.join("param_changes.jsonl"),
            baseline: state.join("fingerprint_baseline.json"),
        })
        .with_timestamp(input.get("timestamp").clone()),
    });
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("multipart fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("multipart fixture already stopped");
        }
    });
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let service = Arc::clone(&service);
                connections.spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let service = Arc::clone(&service);
                        async move {
                            match handle(request, service).await {
                                Ok(response) => Ok::<_, Infallible>(response),
                                Err(error) => {
                                    eprintln!("multipart fixture handler: {error}");
                                    let mut response = Response::new(Full::new(Bytes::new()));
                                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                    Ok(response)
                                }
                            }
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await { eprintln!("multipart fixture connection: {error}"); }
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
        .map_err(|_| "multipart fixture input thread panicked")?;
    Ok(())
}
