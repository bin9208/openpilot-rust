use hyper::{service::service_fn, Method, StatusCode};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    http::{Application, Body},
    params::Backend,
    popular_values::{self, cache, payload, Service},
    settings::Catalog,
    Value,
};
use sha2::{Digest, Sha256};
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::Arc,
};

fn policy(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let state = PathBuf::from(input.get("state").string()?);
    let params = if input.get("root").truth() {
        Backend::native(
            openpilot_params::Params::for_runtime_at(&PathBuf::from(input.get("root").string()?))?,
            state,
        )
    } else {
        Backend::memory(state)
    };
    let catalog = Catalog::from_data(input.get("catalog").clone())?;
    Ok(match input.get("operation").string()?.as_str() {
        "snapshot" => {
            payload::snapshot(&params, &catalog, "owned-fixture-host")?.unwrap_or(Value::Null)
        }
        "hash" => Value::text(&payload::settings_hash(&catalog)?),
        "coerce" => payload::coerce(input.get("value"), input.get("setting"))?,
        "repo_id" => Value::text(&payload::repo_id(&input.get("value").string()?)),
        "device_id" => Value::text(&payload::device_id(&params, "owned-fixture-host")),
        "read" => cache::read(
            Some(input.get("memory")),
            &input.get("car_key").string()?,
            &input.get("settings_hash").string()?,
        )?,
        "store" => cache::store(
            input.get("memory"),
            input.get("now").float()?,
            &input.get("settings_hash").string()?,
        )?,
        "detail" => cache::detail(input.get("memory"), &input.get("name").string()?),
        "schedule" => Value::Bool(cache::should_schedule(
            input.get("session").truth(),
            input.get("now").float()?,
            input.get("last").float()?,
            input.get("interval").float()?,
            input.get("in_flight").truth(),
        )),
        "endpoint" => Value::text(&popular_values::config::endpoint(
            &params,
            input.get("popular").truth(),
        )),
        "credentials_digest" => {
            let (id, secret) = popular_values::config::credentials(&params);
            Value::object([
                (
                    "id",
                    Value::text(&format!("{:x}", Sha256::digest(id.as_bytes()))),
                ),
                (
                    "secret",
                    Value::text(&format!("{:x}", Sha256::digest(secret.as_bytes()))),
                ),
            ])
        }
        "env" => Value::object([
            (
                "timeout",
                Value::Float(
                    popular_values::config::env_float("CARROT_PARAM_VALUE_TIMEOUT_S", 4.).max(1.),
                ),
            ),
            (
                "delay",
                Value::Float(
                    popular_values::config::env_float("CARROT_PARAM_VALUE_RETRY_DELAY_S", 15.)
                        .max(1.),
                ),
            ),
            (
                "attempts",
                Value::integer(
                    popular_values::config::env_int("CARROT_PARAM_VALUE_RETRY_COUNT", 5).max(1),
                ),
            ),
            (
                "interval",
                Value::Float(popular_values::config::env_float(
                    "CARROT_PARAM_VALUE_REFRESH_MIN_S",
                    60.,
                )),
            ),
        ]),
        operation => return Err(format!("unknown fixture operation {operation}").into()),
    })
}

fn response(value: Value) -> Result<hyper::Response<Body>, Box<dyn std::error::Error>> {
    let bytes = value.encode()?;
    Ok(hyper::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json; charset=utf-8")
        .header("content-length", bytes.len())
        .body(http_body_util::Full::new(bytes::Bytes::from(bytes)))?)
}

async fn fixture(
    request: hyper::Request<hyper::body::Incoming>,
    app: Arc<Application>,
    service: Arc<Service>,
) -> Result<hyper::Response<Body>, std::convert::Infallible> {
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let result = match path.as_str() {
        "/__fixture/wait" => service
            .wait_scheduled()
            .await
            .and_then(|()| service.read_application(&app)),
        "/__fixture/boot" => match service.start_upload(Arc::clone(&app)) {
            Some(task) => match task.await {
                Ok(()) => service.read_application(&app),
                Err(error) => Err(openpilot_carrot_server::Error::Source(error.to_string())),
            },
            None => service.read_application(&app),
        },
        "/__fixture/schedule" if request.method() == Method::POST => {
            match openpilot_carrot_server::http::read_json(
                openpilot_carrot_server::http::decode_request(request),
            )
            .await
            {
                Ok(value) => service
                    .schedule(
                        Arc::clone(&app),
                        value.get("now").float().unwrap_or(1000.),
                        Some(value.get("interval").float().unwrap_or(60.)),
                    )
                    .map(Value::Bool),
                Err(error) => Err(error),
            }
        }
        "/__fixture/seed" if request.method() == Method::POST => {
            match openpilot_carrot_server::http::read_json(
                openpilot_carrot_server::http::decode_request(request),
            )
            .await
            {
                Ok(value) => service
                    .seed(Some(value))
                    .and_then(|()| service.read_application(&app)),
                Err(error) => Err(error),
            }
        }
        _ => {
            return Ok(popular_values::handle(
                openpilot_carrot_server::http::decode_request(request),
                app,
                service,
            )
            .await)
        }
    };
    Ok(
        match result.and_then(|value| {
            response(value)
                .map_err(|error| openpilot_carrot_server::Error::Source(error.to_string()))
        }) {
            Ok(response) => response,
            Err(error) => hyper::Response::new(http_body_util::Full::new(bytes::Bytes::from(
                error.to_string(),
            ))),
        },
    )
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
    let state = PathBuf::from(input.get("state").string()?);
    let root = PathBuf::from(input.get("root").string()?);
    let params = if input.get("unavailable").truth() {
        Backend::memory(state.clone())
    } else {
        Backend::native(
            openpilot_params::Params::for_runtime_at(&root)?,
            state.clone(),
        )
    };
    let service = Service::for_test(
        !input.get("no_session").truth(),
        1000.,
        "owned-fixture-host",
    );
    let mut config = Config::at(&state, &state, &state.join("settings.json"));
    if input.get("composed").truth() {
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
    }
    let mut app = Application::new(config, params);
    Arc::get_mut(&mut app)
        .ok_or("fixture Application is already shared")?
        .popular_values = Arc::clone(&service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        let result = io::stdin().lock().read_line(&mut line);
        if let Err(error) = result {
            eprintln!("fixture stop: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("fixture stop receiver closed");
        }
    });
    if input.get("composed").truth() {
        openpilot_carrot_server::http::serve(app, listener, async move {
            if stopped.await.is_err() {
                eprintln!("fixture stop channel closed");
            }
        })
        .await?;
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = Arc::clone(&app);
                let service = Arc::clone(&service);
                connections.spawn(async move {
                    let handler = service_fn(move |request| fixture(request, Arc::clone(&app), Arc::clone(&service)));
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), handler).await { eprintln!("fixture connection: {error}"); }
                });
            }
        }
    }
    service.shutdown().await?;
    connections.abort_all();
    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            if !error.is_cancelled() {
                return Err(error.into());
            }
        }
    }
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
