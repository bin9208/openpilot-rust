use crate::{http::Application, http_route::route, Error};
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
    let background = crate::http_background::Background::start(&app);
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
                let (socket, peer) = match accepted {
                    Ok(socket) => socket,
                    Err(error) => break Err(Error::Io(error)),
                };
                let app = Arc::clone(&app);
                let sound = sound.clone();
                let mut stopped = stop.subscribe();
                connections.spawn(async move {
                    let service = hyper::service::service_fn(move |mut request| {
                        request.extensions_mut().insert(peer);
                        route(request, Arc::clone(&app), sound.clone())
                    });
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
    let mut navi_changes = app.web_navi.as_ref().map(|service| service.subscribe());
    let mut tools_changes = app.tools.as_ref().map(|service| service.changed());
    let mut terminal_changes = app.terminal.as_ref().map(|service| service.changed());
    let mut support_changes = app
        .support_terminal
        .as_ref()
        .map(|service| service.changed());
    app.dashcam_sync_uploads.quiesce();
    if let Some(service) = &app.support_terminal {
        service.quiesce();
    }
    if let Some(service) = &app.terminal {
        service.quiesce();
    }
    if let Some(service) = &app.tools {
        service.quiesce();
    }
    if let Some(service) = &app.live {
        service.quiesce();
    }
    if let Some(service) = &app.web_navi {
        service.quiesce();
    }
    stop.send_replace(true);
    sound_stop.send_replace(crate::web_sound::Shutdown::Quiescing);
    if tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            while let Ok(launch) = launches.try_recv() {
                sounds.spawn_local(crate::web_sound_http::run(launch));
            }
            if connections.is_empty()
                && sounds.is_empty()
                && app.dashcam_sync_uploads.is_idle()
                && app.live.as_ref().is_none_or(|service| service.is_idle())
                && app
                    .web_navi
                    .as_ref()
                    .is_none_or(|service| service.is_idle())
                && app.tools.as_ref().is_none_or(|service| service.is_idle())
                && app
                    .support_terminal
                    .as_ref()
                    .is_none_or(|service| service.is_idle())
                && app
                    .terminal
                    .as_ref()
                    .is_none_or(|service| service.is_idle())
            {
                break;
            }
            tokio::select! {
                _ = connections.join_next(), if !connections.is_empty() => {}
                _ = sounds.join_next(), if !sounds.is_empty() => {}
                _ = sync_changes.changed() => {}
                _ = changed(&mut live_changes), if live_changes.is_some() => {}
                _ = changed(&mut navi_changes), if navi_changes.is_some() => {}
                _ = changed(&mut tools_changes), if tools_changes.is_some() => {}
                _ = changed(&mut terminal_changes), if terminal_changes.is_some() => {}
                _ = changed(&mut support_changes), if support_changes.is_some() => {}
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
        if let Some(service) = &app.tools {
            service.force();
        }
        if let Some(service) = &app.live {
            service.force();
        }
        if let Some(service) = &app.web_navi {
            service.force();
        }
        if let Some(service) = &app.terminal {
            service.force();
        }
        if let Some(service) = &app.support_terminal {
            service.force();
        }
        sound_stop.send_replace(crate::web_sound::Shutdown::Force);
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        while sounds.join_next().await.is_some() {}
    }
    crate::http_shutdown::finish(&app, background).await?;
    served
}

async fn changed<T>(changes: &mut Option<watch::Receiver<T>>) {
    if let Some(changes) = changes {
        if let Err(error) = changes.changed().await {
            eprintln!("Application service notification: {error}");
        }
    }
}
