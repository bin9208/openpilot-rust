use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, Service},
    http::{Application, Body},
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

fn response(value: Value) -> Result<hyper::Response<Body>, Box<dyn std::error::Error>> {
    let body = value.encode()?;
    Ok(hyper::Response::builder()
        .header("content-type", "application/json; charset=utf-8")
        .header("content-length", body.len())
        .body(http_body_util::Full::new(bytes::Bytes::from(body)))?)
}
async fn fixture(
    request: hyper::Request<hyper::body::Incoming>,
    service: Arc<Service>,
) -> Result<hyper::Response<Body>, std::convert::Infallible> {
    if request.uri().path() == "/__fixture/control" {
        let result = openpilot_carrot_server::http::read_json(
            openpilot_carrot_server::http::decode_request(request),
        )
        .await;
        let result = match result {
            Ok(value) => tokio::task::spawn_blocking(
                move || -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(match value.get("operation").string()?.as_str() {
                        "clock" => {
                            service.set_monotonic(value.get("now").float()?)?;
                            Value::Null
                        }
                        "cached" => Value::Array(service.cached_routes()?),
                        "bounds" => {
                            let Value::Array(segments) = value.get("segments") else {
                                return Err("segments must be an array".into());
                            };
                            let (start, end) = service.catalog.route_time_bounds(segments)?;
                            Value::Array(vec![Value::integer(start), Value::integer(end)])
                        }
                        _ => return Err("unknown fixture control".into()),
                    })
                },
            )
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string())),
            Err(error) => Err(error.to_string()),
        };
        let value = match result {
            Ok(value) => Value::object([("value", value)]),
            Err(error) => Value::object([("error", Value::text(&error))]),
        };
        return Ok(response(value).unwrap_or_else(|error| {
            hyper::Response::new(http_body_util::Full::new(bytes::Bytes::from(
                error.to_string(),
            )))
        }));
    }
    Ok(dashcam::handle(
        openpilot_carrot_server::http::decode_request(request),
        service,
    )
    .await)
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let service = Service::for_test(
        PathBuf::from(input.get("root").string()?),
        PathBuf::from(input.get("state").string()?),
        Some(input.get("wall").int()?.to_i64().ok_or("wall too large")?),
        Some(input.get("monotonic").float()?),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut line) {
            eprintln!("fixture input: {error}");
        }
        let _ = stop.send(());
    });
    if input.get("composed").truth() {
        let state = PathBuf::from(input.get("app_state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        std::fs::create_dir_all(&config.web)?;
        std::fs::create_dir_all(&config.shared_assets)?;
        let mut app = Application::new(config, Backend::memory(state));
        let app_mut = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        app_mut.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        app_mut.dashcam = Arc::clone(&service);
        openpilot_carrot_server::http::serve(app, listener, async move {
            let _ = stopped.await;
        })
        .await?;
    } else {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                accepted = listener.accept() => {
                    let (stream,_) = accepted?; let service = Arc::clone(&service);
                    connections.spawn(async move { let handler = service_fn(move |request| fixture(request,Arc::clone(&service))); if let Err(error) = hyper::server::conn::http1::Builder::new().close_after_response(true).serve_connection(TokioIo::new(stream),handler).await { eprintln!("fixture connection: {error}"); } });
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
    }
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
