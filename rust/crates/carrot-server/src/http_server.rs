use crate::{
    http::{route, Application},
    Error,
};
use hyper_util::rt::TokioIo;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::watch, task::JoinSet};

pub async fn serve(
    app: Arc<Application>,
    listener: TcpListener,
    shutdown: impl Future<Output = ()>,
) -> Result<(), Error> {
    tokio::task::LocalSet::new()
        .run_until(serve_local(app, listener, shutdown))
        .await
}

async fn serve_local(
    app: Arc<Application>,
    listener: TcpListener,
    shutdown: impl Future<Output = ()>,
) -> Result<(), Error> {
    app.config.validate()?;
    app.static_web.validate()?;
    app.config.migrate_legacy_state();
    let (heartbeat_stop, heartbeat_stopped) = watch::channel(false);
    let heartbeat = app.heartbeat_params.clone().map(|params| {
        let service = Arc::clone(&app.heartbeat);
        let backend = crate::params::Backend::native(params, app.config.state.clone());
        tokio::spawn(async move { service.run_loop(backend, heartbeat_stopped).await })
    });
    let (git_stop, git_stopped) = watch::channel(false);
    let git_status = app
        .git_status
        .clone()
        .map(|service| tokio::spawn(async move { service.run_loop(git_stopped).await }));
    let (auto_stop, auto_stopped) = watch::channel(false);
    let auto_update = app
        .auto_update
        .clone()
        .map(|service| tokio::task::spawn_local(async move { service.run(auto_stopped).await }));
    let precompress = app.static_web.start_precompress();
    let popular_upload = app.popular_values.start_upload(Arc::clone(&app));
    let warm_app = Arc::clone(&app);
    let warm = tokio::task::spawn_blocking(move || {
        let _ = warm_app.settings_payload();
    });
    let (stop, _) = watch::channel(false);
    let (sound_stop, _) = watch::channel(crate::web_sound::Shutdown::Running);
    let (sound, mut launches) = crate::web_sound_http::Sessions::channel(sound_stop.subscribe());
    let mut connections = JoinSet::new();
    let mut sounds = JoinSet::new();
    tokio::pin!(shutdown);
    let served = loop {
        tokio::select! {
            () = &mut shutdown => break Ok(()),
            accepted = listener.accept() => {
                let (socket, _) = match accepted {
                    Ok(socket) => socket,
                    Err(error) => break Err(Error::Io(error)),
                };
                let app = Arc::clone(&app);
                let sound = sound.clone();
                let mut stopped = stop.subscribe();
                connections.spawn(async move {
                    let service = hyper::service::service_fn(move |request| route(request, Arc::clone(&app), sound.clone()));
                    let connection = hyper::server::conn::http1::Builder::new()
                        .preserve_raw_conditional_headers(true)
                        .close_after_response(true)
                        .prefetch_buffered_body(true)
                        .application_continue(true)
                        .read_buf_exact_size(256 * 1024)
                        .serve_connection(TokioIo::new(socket), service)
                        .with_upgrades();
                    tokio::pin!(connection);
                    tokio::select! {
                        result = &mut connection => result,
                        _ = stopped.changed() => { connection.as_mut().graceful_shutdown(); connection.await }
                    }
                });
            }
            Some(launch) = launches.recv() => {
                sounds.spawn_local(crate::web_sound_http::run(launch));
            }
            joined = sounds.join_next(), if !sounds.is_empty() => {
                if let Some(Ok(Err(error))) = joined {
                    eprintln!("Web Sound session: {error}");
                }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    };
    drop(listener);
    let mut sync_changes = app.dashcam_sync_uploads.subscribe();
    let mut live_changes = app.live.as_ref().map(|service| service.subscribe());
    app.dashcam_sync_uploads.quiesce();
    if let Some(service) = &app.live {
        service.quiesce();
    }
    let _ = stop.send(true);
    let _ = sound_stop.send(crate::web_sound::Shutdown::Quiescing);
    if tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            while let Ok(launch) = launches.try_recv() {
                sounds.spawn_local(crate::web_sound_http::run(launch));
            }
            if connections.is_empty() && sounds.is_empty() && app.dashcam_sync_uploads.is_idle()
                && app.live.as_ref().is_none_or(|service| service.is_idle()) {
                break;
            }
            tokio::select! {
                _ = connections.join_next(), if !connections.is_empty() => {}
                _ = sounds.join_next(), if !sounds.is_empty() => {}
                _ = sync_changes.changed() => {}
                _ = async { if let Some(changes) = &mut live_changes { let _ = changes.changed().await; } }, if live_changes.is_some() => {}
                Some(launch) = launches.recv() => {
                    sounds.spawn_local(crate::web_sound_http::run(launch));
                }
            }
        }
    })
    .await
    .is_err()
    {
        app.dashcam_sync_uploads.force();
        if let Some(service) = &app.live { service.force(); }
        let _ = sound_stop.send(crate::web_sound::Shutdown::Force);
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        while sounds.join_next().await.is_some() {}
    }
    app.dashcam_sync_uploads.shutdown().await?;
    if let Some(service) = &app.live {
        service.shutdown().await?;
    }
    warm.abort();
    heartbeat_stop.send_replace(true);
    if let Some(task) = heartbeat {
        match task.await {
            Ok(Ok(
                crate::heartbeat::LoopExit::Returned | crate::heartbeat::LoopExit::Cancelled,
            )) => {}
            Ok(Err(error)) => eprintln!("heartbeat cleanup: {error}"),
            Err(error) => eprintln!("heartbeat task: {error}"),
        }
    }
    git_stop.send_replace(true);
    if let Some(task) = git_status {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("Git status cleanup: {error}"),
            Err(error) => eprintln!("Git status task: {error}"),
        }
    }
    auto_stop.send_replace(true);
    if let Some(task) = auto_update {
        if let Err(error) = task.await {
            eprintln!("Auto update cleanup: {error}");
        }
    }
    if let Some(task) = popular_upload {
        task.abort();
        match task.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => eprintln!("popular startup refresh: {error}"),
        }
    }
    app.popular_values.shutdown().await?;
    let _ = warm.await;
    precompress.abort();
    match precompress.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("static precompression: {error}"),
        Err(error) if error.is_cancelled() => {}
        Err(error) => eprintln!("static precompression task: {error}"),
    }
    app.bluetooth_http.shutdown().await?;
    served
}
