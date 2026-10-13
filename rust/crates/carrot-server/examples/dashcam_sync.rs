use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    config::Config,
    dashcam::{self, SyncUploads, Uploads},
    http::{decode_request, Application},
    params::Backend,
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{oneshot, watch},
    task::JoinSet,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input = Value::parse(&line)?;
    let root = PathBuf::from(input.get("root").string()?);
    let worker = PathBuf::from(input.get("worker").string()?);
    let settings = if matches!(input.get("settings"), Value::Null) {
        None
    } else {
        Some(serde_json::from_str(&input.get("settings").encode()?)?)
    };
    let uploads = SyncUploads::for_test(root.clone(), worker.clone(), settings.clone());
    let jobs = Uploads::for_test(root, worker, settings);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        Value::object([
            ("port", Value::integer(listener.local_addr()?.port())),
            ("pid", Value::integer(std::process::id())),
        ])
        .encode()?
    );
    io::stdout().flush()?;
    let (stop, mut stopped) = oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let mut command = String::new();
        if let Err(error) = io::stdin().lock().read_line(&mut command) {
            eprintln!("fixture input: {error}");
        }
        drop(stop.send(command));
    });
    if input.get("composed").truth() {
        let state = PathBuf::from(input.get("state").string()?);
        let mut config = Config::at(&state, &state, &state.join("settings.json"));
        config.repository = PathBuf::from(input.get("repository").string()?);
        config.web = state.join("owned-web");
        config.shared_assets = state.join("owned-assets");
        config.legacy_state = state.join("absent-legacy");
        config.params_backup = state.join("owned-backup.json");
        std::fs::create_dir_all(&config.web)?;
        std::fs::create_dir_all(&config.shared_assets)?;
        let mut app = Application::new(config, Backend::memory(state));
        let owned = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
        owned.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        owned.dashcam_sync_uploads = Arc::clone(&uploads);
        owned.dashcam_uploads = jobs;
        openpilot_carrot_server::http::serve(app, listener, async move {
            drop(stopped.await);
        })
        .await?;
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let (quiesce, _) = watch::channel(false);
    let mut connections = JoinSet::new();
    let command = loop {
        tokio::select! {
            command = &mut stopped => break command?.trim().to_owned(),
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let uploads = Arc::clone(&uploads);
                let jobs = Arc::clone(&jobs);
                let mut quiesced = quiesce.subscribe();
                connections.spawn(async move {
                    let handler = service_fn(move |request| {
                        let uploads = Arc::clone(&uploads);
                        let jobs = Arc::clone(&jobs);
                        async move {
                            let request = decode_request(request);
                            let response = if dashcam::sync_upload_matches(request.uri().path()) {
                                dashcam::sync_upload_handle(request, uploads).await
                            } else {
                                dashcam::upload_handle(request, jobs).await
                            };
                            Ok::<_, std::convert::Infallible>(response)
                        }
                    });
                    let connection = hyper::server::conn::http1::Builder::new()
                        .preserve_raw_conditional_headers(true).close_after_response(true)
                        .serve_connection(TokioIo::new(socket), handler);
                    tokio::pin!(connection);
                    let result = tokio::select! {
                        result = &mut connection => result,
                        _ = quiesced.changed() => {
                            connection.as_mut().graceful_shutdown(); connection.await
                        }
                    };
                    if let Err(error) = result { eprintln!("fixture connection: {error}"); }
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    };
    drop(listener);
    if command == "drop" {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        drop(uploads);
        drop(jobs);
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let mut changes = uploads.subscribe();
    uploads.quiesce();
    quiesce.send_replace(true);
    let expired = {
        let grace = async {
            loop {
                if connections.is_empty() && uploads.is_idle() {
                    break;
                }
                tokio::select! {
                    _ = connections.join_next(), if !connections.is_empty() => {}
                    _ = changes.changed() => {}
                }
            }
        };
        command == "force"
            || tokio::time::timeout(Duration::from_secs(60), grace)
                .await
                .is_err()
    };
    if expired {
        uploads.force();
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    uploads.shutdown().await?;
    drop(jobs);
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
