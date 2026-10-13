use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    bluetooth_http::{self, Service},
    http::decode_request,
    Value,
};
use std::{
    convert::Infallible,
    io::{self, BufRead, Write},
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let bus = input.get("bus").string()?;
    if input.get("reader_control").truth() {
        let mut client = openpilot_bluetooth::bluez::Bluez::new(Some(bus));
        let reader = client.snapshot_reader().await?;
        let before = reader.snapshot().await?;
        client.close().await?;
        let error = match reader.snapshot().await {
            Ok(_) => return Err("closed reader succeeded".into()),
            Err(error) => error.to_string(),
        };
        let after = client.snapshot().await?;
        client.close().await?;
        println!(
            "{}",
            serde_json::json!({"before": before, "old_reader_error": error, "after": after})
        );
        return Ok(());
    }
    let service = Service::at(
        input.get("config").string()?.into(),
        input.get("runtime").string()?.into(),
        input.get("command").string()?.into(),
        Some(bus),
        input
            .has("timestamp")
            .then(|| input.get("timestamp").float())
            .transpose()?,
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    println!(
        "{}",
        Value::object([("port", Value::integer(listener.local_addr()?.port()))]).encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("Bluetooth fixture stdin: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("Bluetooth fixture already stopped");
        }
    });
    if input.get("composed").truth() {
        let runtime: std::path::PathBuf = input.get("runtime").string()?.into();
        let root = runtime.parent().ok_or("owned fixture root required")?;
        let mut config = openpilot_carrot_server::config::Config::at(
            &root.join("repository"),
            &root.join("data"),
            &root.join("settings.json"),
        );
        config.legacy_state = root.join("legacy");
        std::fs::create_dir_all(&config.web)?;
        std::fs::create_dir_all(&config.shared_assets)?;
        let backend = openpilot_carrot_server::params::Backend::memory(config.state.clone());
        let mut app = openpilot_carrot_server::http::Application::new(config, backend);
        let owned = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        owned.bluetooth_http = service;
        owned.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        openpilot_carrot_server::http::serve(app, listener, async move {
            let _ = stopped.await;
        })
        .await?;
        worker
            .join()
            .map_err(|_| "Bluetooth fixture input thread panicked")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let service = Arc::clone(&service);
                connections.spawn(async move {
                    let handler = service_fn(move |request: Request<Incoming>| {
                        let service = Arc::clone(&service);
                        async move { Ok::<_, Infallible>(bluetooth_http::handle(decode_request(request), service).await) }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().close_after_response(true).application_continue(true).prefetch_buffered_body(true).read_buf_exact_size(256 * 1024).serve_connection(TokioIo::new(stream), handler).await { eprintln!("Bluetooth fixture connection: {error}"); }
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
    service.shutdown().await?;
    worker
        .join()
        .map_err(|_| "Bluetooth fixture input thread panicked")?;
    Ok(())
}
