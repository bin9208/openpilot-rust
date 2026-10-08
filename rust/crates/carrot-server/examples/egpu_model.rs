use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    egpu_model::{self, ModelFiles},
    http::Application,
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
    let backend = if input.get("unavailable").truth() {
        Backend::memory(config.state.clone())
    } else {
        Backend::native(
            openpilot_params::Params::for_runtime_at(&path("params")?)?,
            config.state.clone(),
        )
    };
    let app = Application::new(config, backend);
    let files = Arc::new(ModelFiles {
        paths: openpilot_usbgpu::model::Paths {
            models: path("models")?,
            cache: path("model_cache")?,
        },
        usb_devices: path("usb_devices")?,
        timestamp: input
            .has("egpu_timestamp")
            .then(|| input.get("egpu_timestamp").float())
            .transpose()?,
    });
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("eGPU fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("eGPU fixture already stopped");
        }
    });
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = Arc::clone(&app);
                let files = Arc::clone(&files);
                connections.spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let app = Arc::clone(&app);
                        let files = Arc::clone(&files);
                        async move {
                            let path = percent_encoding::percent_decode_str(request.uri().path()).decode_utf8_lossy().into_owned();
                            Ok::<_, Infallible>(egpu_model::handle(&request, app, files, &path).await)
                        }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await {
                        eprintln!("eGPU fixture connection: {error}");
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
    worker
        .join()
        .map_err(|_| "eGPU fixture input thread panicked")?;
    Ok(())
}
