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
    app.config.validate()?;
    app.config.migrate_legacy_state();
    let warm_app = Arc::clone(&app);
    let warm = tokio::task::spawn_blocking(move || {
        let _ = warm_app.settings_payload();
    });
    let (stop, _) = watch::channel(false);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let app = Arc::clone(&app);
                let mut stopped = stop.subscribe();
                connections.spawn(async move {
                    let service = hyper::service::service_fn(move |request| route(request, Arc::clone(&app)));
                    let connection = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket), service);
                    tokio::pin!(connection);
                    tokio::select! {
                        result = &mut connection => result,
                        _ = stopped.changed() => { connection.as_mut().graceful_shutdown(); connection.await }
                    }
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    drop(listener);
    let _ = stop.send(true);
    if tokio::time::timeout(Duration::from_secs(60), async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    warm.abort();
    let _ = warm.await;
    Ok(())
}
