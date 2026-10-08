use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    config::Config,
    http::{Application, Body},
    params::Backend,
    screenrecord::{self, catalog, Screenrecord},
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

fn service(input: &Value) -> Result<Arc<Screenrecord>, Box<dyn std::error::Error>> {
    let Value::Array(directories) = input.get("directories") else {
        return Err("directories must be an array".into());
    };
    Ok(Screenrecord::for_test(
        directories
            .iter()
            .map(|value| value.string().map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()?,
        PathBuf::from(input.get("cache").string()?),
        PathBuf::from(input.get("ffmpeg").string()?),
        Some(input.get("wall").int()?.to_i64().ok_or("wall too large")?),
        Some(input.get("monotonic").float()?),
    ))
}
fn policy(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let integer = |name: &str| {
        input.get(name).int()?.to_i64().ok_or_else(|| {
            openpilot_carrot_server::Error::Source("fixture integer out of range".into())
        })
    };
    Ok(match input.get("operation").string()?.as_str() {
        "date" => Value::text(&catalog::date_label(integer("epoch")?)),
        "relative" => Value::text(&catalog::relative_time(integer("epoch")?, integer("wall")?)),
        "file_id" => Value::text(&catalog::file_id(&PathBuf::from(
            input.get("path").string()?,
        ))?),
        "token" => Value::text(&catalog::token(input.get("id"))),
        "catalog" => Value::Array(service(input)?.build_videos()),
        "find" => Value::text(
            &service(input)?
                .find_file(&input.get("id").string()?)?
                .to_string_lossy(),
        ),
        _ => return Err("unknown fixture policy".into()),
    })
}
fn response(value: Value) -> Result<hyper::Response<Body>, Box<dyn std::error::Error>> {
    let body = value.encode()?;
    Ok(hyper::Response::builder()
        .header("content-type", "application/json; charset=utf-8")
        .header("content-length", body.len())
        .body(http_body_util::Full::new(bytes::Bytes::from(body)))?)
}
async fn fixture(
    request: hyper::Request<hyper::body::Incoming>,
    service: Arc<Screenrecord>,
) -> Result<hyper::Response<Body>, std::convert::Infallible> {
    if request.uri().path() == "/__fixture/clock" {
        let result = openpilot_carrot_server::http::read_json(
            openpilot_carrot_server::http::decode_request(request),
        )
        .await
        .and_then(|value| value.get("now").float().map_err(Into::into))
        .and_then(|now| {
            service
                .set_monotonic(now)
                .map_err(|error| openpilot_carrot_server::Error::Source(error.to_string()))
        });
        let value = match result {
            Ok(()) => Value::object([("ok", Value::Bool(true))]),
            Err(error) => Value::object([("error", Value::text(&error.to_string()))]),
        };
        return Ok(response(value).unwrap_or_else(|error| {
            hyper::Response::new(http_body_util::Full::new(bytes::Bytes::from(
                error.to_string(),
            )))
        }));
    }
    Ok(screenrecord::handle(
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
    if !input.get("http").truth() {
        let result = match policy(&input) {
            Ok(value) => Value::object([("value", value)]),
            Err(error) => Value::object([("error", Value::text(&error.to_string()))]),
        };
        println!("{}", result.encode()?);
        return Ok(());
    }
    let service = service(&input)?;
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
        let state = PathBuf::from(input.get("state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        let mut app = Application::new(config, Backend::memory(state));
        let app_mut = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        app_mut.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        app_mut.screenrecord = Arc::clone(&service);
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
                    let (stream, _) = accepted?; let service = Arc::clone(&service);
                    connections.spawn(async move { let handler = service_fn(move |request| fixture(request, Arc::clone(&service))); if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), handler).await { eprintln!("fixture connection: {error}"); } });
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
