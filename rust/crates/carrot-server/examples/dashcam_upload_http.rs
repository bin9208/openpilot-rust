use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, Uploads},
    http::Application,
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let settings = if matches!(input.get("settings"), Value::Null) {
        None
    } else {
        Some(serde_json::from_str(&input.get("settings").encode()?)?)
    };
    let uploads = Uploads::for_test(
        PathBuf::from(input.get("root").string()?),
        PathBuf::from(input.get("worker").string()?),
        settings,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([
            ("port", Value::integer(listener.local_addr()?.port())),
            ("pid", Value::integer(std::process::id()))
        ])
        .encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut line = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut line) {
            eprintln!("fixture input: {error}");
        }
        if stop.send(()).is_err() {
            eprintln!("fixture shutdown receiver closed");
        }
    });
    if input.get("composed").truth() {
        let state = PathBuf::from(input.get("state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        std::fs::create_dir_all(&config.web)?;
        std::fs::create_dir_all(&config.shared_assets)?;
        let mut app = Application::new(config, Backend::memory(state));
        let app_mut =
            std::sync::Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        app_mut.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        app_mut.dashcam_uploads = std::sync::Arc::clone(&uploads);
        openpilot_carrot_server::http::serve(app, listener, async move {
            let _ = stopped.await;
        })
        .await?;
        drop(uploads);
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream,_) = accepted?;
                let uploads = std::sync::Arc::clone(&uploads);
                connections.spawn(async move {
                    let handler = service_fn(move |request| {
                        let uploads = std::sync::Arc::clone(&uploads);
                        async move { Ok::<_,std::convert::Infallible>(dashcam::upload_handle(openpilot_carrot_server::http::decode_request(request), uploads).await) }
                    });
                    if let Err(error) = hyper::server::conn::http1::Builder::new().preserve_raw_conditional_headers(true).close_after_response(true).serve_connection(TokioIo::new(stream),handler).await {
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
    drop(uploads);
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
