use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, service::service_fn, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    cars::{self, Cars},
    Value,
};
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
    let directory = PathBuf::from(input.get("supported_cars").string()?);
    let service = if input.has("catalogs") {
        let Value::Array(catalogs) = input.get("catalogs") else {
            return Err("fixture catalogs must be an array".into());
        };
        let catalogs: Vec<String> = catalogs
            .iter()
            .map(Value::string)
            .collect::<Result<_, _>>()?;
        let catalogs: [String; 7] = catalogs
            .try_into()
            .map_err(|_| "fixture requires seven catalogs")?;
        Cars::with_catalogs_for_fixture(directory, catalogs)
    } else {
        Cars::at(directory)
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("cars fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("cars fixture already stopped");
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
                            if request.uri().path() != "/api/cars" {
                                let body = if request.method() == Method::HEAD { Bytes::new() } else { Bytes::from_static(b"404: Not Found") };
                                let mut response = Response::new(Full::new(body));
                                *response.status_mut() = StatusCode::NOT_FOUND;
                                response.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("text/plain; charset=utf-8"));
                                response.headers_mut().insert(hyper::header::CONTENT_LENGTH, hyper::header::HeaderValue::from_static("14"));
                                return Ok::<_, Infallible>(response);
                            }
                            Ok::<_, Infallible>(cars::handle(&request, service).await)
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await {
                        eprintln!("cars fixture connection: {error}");
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
        .map_err(|_| "cars fixture input thread panicked")?;
    Ok(())
}
