use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use openpilot_carrot_server::{
    tools::{
        config::{Config, Paths},
        service::Service,
    },
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

pub async fn run(input: Value) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(input.get("owned_root").string()?);
    if !root.starts_with(std::env::current_dir()?) {
        return Err("fixture root is not owned".into());
    }
    let params = if input.get("params_root").truth() {
        let path = PathBuf::from(input.get("params_root").string()?);
        if !path.starts_with(&root) {
            return Err("Params root is not owned".into());
        }
        Some(openpilot_params::Params::open(
            &path,
            &input.get("prefix").string()?,
        )?)
    } else {
        None
    };
    let config = Config {
        paths: Paths {
            repository: root.join("repository"),
            lock: root.join("repository.lock"),
            launcher: PathBuf::from(input.get("launcher").string()?),
            videos: root.join("media/0/videos"),
            logs: root.join("media/0/realdata"),
            calibration: [
                root.join("owned-params/d_tmp/CalibrationParams"),
                root.join("owned-params/d/CalibrationParams"),
            ],
            tmux_log: root.join("media/tmux.log"),
            backup: root.join("media/params_backup.json"),
            history: root.join("state/tool_jobs.json"),
        },
        params,
        git_status: None,
        auto_update: None,
    };
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
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let input_worker = std::thread::spawn(move || {
        let _line = io::stdin().lock().lines().next();
        let _accepted = stop.send(());
    });
    if input.get("application").truth() {
        super::application::run(
            super::application::Fixture { root, config },
            listener,
            stopped,
        )
        .await?;
        input_worker.join().map_err(|_| "fixture input failed")?;
        return Ok(());
    }
    let service = Service::new(config);
    let (quiescing, _) = tokio::sync::watch::channel(false);
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let service = Arc::clone(&service);
                let mut stop = quiescing.subscribe();
                connections.spawn(async move {
                    let handler = service_fn(move |request| {
                        let service = Arc::clone(&service);
                        async move {
                            Ok::<_, std::convert::Infallible>(openpilot_carrot_server::tools::http::handle(
                                openpilot_carrot_server::http::decode_request(request), service,
                            ).await)
                        }
                    });
                    let connection = hyper::server::conn::http1::Builder::new()
                        .preserve_raw_conditional_headers(true).close_after_response(true)
                        .prefetch_buffered_body(true).application_continue(true)
                        .read_buf_exact_size(256 * 1024).serve_connection(TokioIo::new(stream), handler);
                    tokio::pin!(connection);
                    tokio::select! {
                        result = &mut connection => result,
                        _ = stop.changed() => {
                            connection.as_mut().graceful_shutdown();
                            connection.await
                        }
                    }
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    drop(listener);
    let mut changed = service.changed();
    service.quiesce();
    if input.get("force").truth() {
        service.force();
    }
    quiescing.send_replace(true);
    if tokio::time::timeout(Duration::from_secs(60), async {
        while !connections.is_empty() || !service.is_idle() {
            tokio::select! {
                _ = connections.join_next(), if !connections.is_empty() => {}
                _ = changed.changed() => {}
            }
        }
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    let observed = Value::object([
        ("jobs", service.jobs.snapshots(20)?),
        ("app_runner_cleanup_finished", Value::Bool(true)),
    ]);
    std::fs::write(root.join("app-cleanup.json"), observed.encode()?)?;
    service.shutdown().await?;
    input_worker.join().map_err(|_| "fixture input failed")?;
    Ok(())
}
