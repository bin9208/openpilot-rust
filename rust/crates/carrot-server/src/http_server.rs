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
    let precompress = app.static_web.start_precompress();
    let warm_app = Arc::clone(&app);
    let warm = tokio::task::spawn_blocking(move || {
        let _ = warm_app.settings_payload();
    });
    let (stop, _) = watch::channel(false);
    let (sound, mut launches) = crate::web_sound_http::Sessions::channel(stop.subscribe());
    let mut connections = JoinSet::new();
    let mut sounds = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let app = Arc::clone(&app);
                let sound = sound.clone();
                let mut stopped = stop.subscribe();
                connections.spawn(async move {
                    let service = hyper::service::service_fn(move |request| route(request, Arc::clone(&app), sound.clone()));
                    let connection = hyper::server::conn::http1::Builder::new()
                        .preserve_raw_conditional_headers(true)
                        .close_after_response(true)
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
    }
    drop(listener);
    let _ = stop.send(true);
    if tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            while let Ok(launch) = launches.try_recv() {
                sounds.spawn_local(crate::web_sound_http::run(launch));
            }
            if connections.is_empty() && sounds.is_empty() {
                break;
            }
            tokio::select! {
                _ = connections.join_next(), if !connections.is_empty() => {}
                _ = sounds.join_next(), if !sounds.is_empty() => {}
                Some(launch) = launches.recv() => {
                    sounds.spawn_local(crate::web_sound_http::run(launch));
                }
            }
        }
    })
    .await
    .is_err()
    {
        connections.abort_all();
        sounds.abort_all();
        while connections.join_next().await.is_some() {}
        while sounds.join_next().await.is_some() {}
    }
    warm.abort();
    let _ = warm.await;
    precompress.abort();
    match precompress.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("static precompression: {error}"),
        Err(error) if error.is_cancelled() => {}
        Err(error) => eprintln!("static precompression task: {error}"),
    }
    Ok(())
}
