use bytes::Bytes;
use http_body_util::Full;
use hyper::{body::Incoming, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    config::Config,
    http::Application,
    intro::{self, Intro, Route},
    params::Backend,
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
    let path = |name| -> Result<PathBuf, openpilot_carrot_navi::Error> {
        Ok(input.get(name).string()?.into())
    };
    let config = Config::at(&path("repository")?, &path("data")?, &path("settings")?);
    let backend = if input.get("has_params").truth() {
        Backend::native(
            openpilot_params::Params::for_runtime_at(&path("params")?)?,
            config.state.clone(),
        )
    } else {
        Backend::memory(config.state.clone())
    };
    let intro = if input.has("timestamp") {
        Intro::with_timestamp(
            config.clone(),
            input
                .get("timestamp")
                .int()?
                .to_i64()
                .ok_or("fixture timestamp out of range")?,
        )
    } else {
        Intro::new(config.clone())
    };
    let app = Application::new(config, backend);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("intro fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("intro fixture already stopped");
        }
    });
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = Arc::clone(&app);
                let intro = Arc::clone(&intro);
                connections.spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let app = Arc::clone(&app);
                        let intro = Arc::clone(&intro);
                        async move {
                            let response = if let Some(route) = Route::from_path(request.uri().path()) {
                                intro::handle(openpilot_carrot_server::http::decode_request(request), app, intro, route).await
                            } else {
                                let mut response = Response::new(Full::new(Bytes::from_static(b"404: Not Found")));
                                *response.status_mut() = StatusCode::NOT_FOUND;
                                response
                            };
                            Ok::<_, Infallible>(response)
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await { eprintln!("intro fixture connection: {error}"); }
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
        .map_err(|_| "intro fixture input thread panicked")?;
    Ok(())
}
