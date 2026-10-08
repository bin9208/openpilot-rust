use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, MetadataFiles, Service},
    http::Application,
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let service = Service::for_test(
        PathBuf::from(input.get("root").string()?),
        PathBuf::from(input.get("state").string()?),
        Some(input.get("wall").int()?.to_i64().ok_or("wall too large")?),
        Some(1.),
    );
    let Value::Array(mime_files) = input.get("mime_files") else {
        return Err("mime_files must be an array".into());
    };
    let mime_files = mime_files
        .iter()
        .map(|value| value.string().map(PathBuf::from))
        .collect::<Result<Vec<_>, _>>()?;
    let files = MetadataFiles::for_test(Arc::clone(&service), mime_files);
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
        app_mut.dashcam_metadata = Arc::clone(&files);
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
                    let (stream,_) = accepted?;let files = Arc::clone(&files);
                    connections.spawn(async move {
                        let handler = service_fn(move |request| { let files = Arc::clone(&files); async move { Ok::<_,std::convert::Infallible>(dashcam::metadata_handle(openpilot_carrot_server::http::decode_request(request),files).await) } });
                        if let Err(error) = hyper::server::conn::http1::Builder::new().preserve_raw_conditional_headers(true).close_after_response(true).serve_connection(TokioIo::new(stream),handler).await { eprintln!("fixture connection: {error}"); }
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
    }
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
