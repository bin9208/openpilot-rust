use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, UploadHealth},
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
    let state = PathBuf::from(input.get("state").string()?);
    let repository = PathBuf::from(input.get("repository").string()?);
    let mut config = Config::at(&repository, &state, &state.join("settings.json"));
    config.web = state.join("owned-web");
    config.shared_assets = state.join("owned-assets");
    config.legacy_state = state.join("absent-legacy");
    config.params_backup = state.join("owned-backup.json");
    std::fs::create_dir_all(&config.web)?;
    std::fs::create_dir_all(&config.shared_assets)?;
    let health = UploadHealth::original(&config, None);
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
        let mut app = Application::new(config, Backend::memory(state));
        std::sync::Arc::get_mut(&mut app)
            .ok_or("fixture Application already shared")?
            .popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        openpilot_carrot_server::http::serve(app, listener, async move {
            let _ = stopped.await;
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
                let (stream,_) = accepted?;
                let health = std::sync::Arc::clone(&health);
                connections.spawn(async move {
                    let handler = service_fn(move |request| {
                        let health = std::sync::Arc::clone(&health);
                        async move { Ok::<_,std::convert::Infallible>(dashcam::health_handle(openpilot_carrot_server::http::decode_request(request),health).await) }
                    });
                    hyper::server::conn::http1::Builder::new()
                        .preserve_raw_conditional_headers(true)
                        .close_after_response(true)
                        .prefetch_buffered_body(true)
                        .application_continue(true)
                        .read_buf_exact_size(256*1024)
                        .serve_connection(TokioIo::new(stream),handler).await
                });
            }
            _ = connections.join_next(),if !connections.is_empty() => {}
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
