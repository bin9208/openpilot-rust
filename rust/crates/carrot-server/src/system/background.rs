use crate::{http::Application, Error};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

pub async fn run(app: Arc<Application>, mut stopped: watch::Receiver<bool>) {
    loop {
        if *stopped.borrow() {
            return;
        }
        let target = Arc::clone(&app);
        let mut update = tokio::task::spawn_blocking(move || {
            let probe = target.system.network.probe();
            let params = target
                .params
                .lock()
                .map_err(|_| Error::Source("Params lock poisoned".into()))?;
            target.system.network.publish(probe, &params)
        });
        let result = tokio::select! {
            result = &mut update => result,
            _ = stopped.changed() => { update.abort(); return; },
        };
        match result {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => println!("[device_network] background refresh failed: {error}"),
            Err(error) => println!("[device_network] background refresh failed: {error}"),
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(15)) => {},
            _ = stopped.changed() => return,
        }
    }
}
