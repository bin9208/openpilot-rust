pub(crate) mod allocator;
use crate::http::Application;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

pub(crate) async fn run(app: Arc<Application>, mut stop: watch::Receiver<bool>) {
    let params = app
        .params
        .lock()
        .ok()
        .and_then(|params| params.native_params().cloned());
    loop {
        if *stop.borrow() {
            return;
        }
        tokio::select! {
            _ = stop.changed() => return,
            () = tokio::time::sleep(Duration::from_secs(30)) => {},
        }
        let raw = match &app.live {
            Some(service) => tokio::select! {
                _ = stop.changed() => return,
                count = service.raw_clients() => count.unwrap_or(0),
            },
            None => 0,
        };
        if raw > 0 {
            continue;
        }
        if params
            .as_ref()
            .is_some_and(|params| params.get_bool("CarrotVisionActive").unwrap_or(false))
        {
            continue;
        }
        if let Err(error) = tokio::task::spawn_blocking(allocator::trim).await {
            eprintln!("Idle allocator trim: {error}");
        }
    }
}
