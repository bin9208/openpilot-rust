use crate::{
    bus::{self, Address},
    engine::Engine,
    settings,
    state::State,
    Error,
};
use openpilot_logging::record::Level;
use std::time::Duration;
use tokio::task::JoinSet;

async fn supervise(
    mut workers: JoinSet<Result<(), Error>>,
    setup: impl std::future::Future<Output = ()>,
    mut stopping: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Error> {
    let mut setup = std::pin::pin!(setup);
    let mut setup_done = false;
    let result = loop {
        tokio::select! {
            biased;
            _ = stopping.wait_for(|stop| *stop) => break Ok(()),
            result = workers.join_next() => break match result {
                Some(Ok(Err(error))) => Err(error),
                Some(Err(_)) => Err(Error::Panicked),
                Some(Ok(Ok(()))) | None => Err(Error::WorkerStopped),
            },
            _ = &mut setup, if !setup_done => setup_done = true,
        }
    };
    workers.abort_all();
    while let Some(result) = workers.join_next().await {
        if let Err(error) = result {
            if !error.is_cancelled() {
                eprintln!("Wi-Fi worker cleanup: {error}");
            }
        }
    }
    result
}

impl Engine {
    async fn adapter(&self) -> Result<Option<String>, Error> {
        let (paths,): (Vec<dbus::Path<'static>>,) = self
            .main
            .call(
                Address {
                    path: bus::NM_PATH,
                    interface: bus::NM,
                },
                "GetDevices",
                (),
            )
            .await?;
        for path in paths {
            let value = self
                .main
                .get(
                    Address {
                        path: &path,
                        interface: bus::DEVICE,
                    },
                    "DeviceType",
                )
                .await?;
            if bus::unsigned(&value, "DeviceType")? == 2 {
                return Ok(Some(path.to_string()));
            }
        }
        Ok(None)
    }
    pub async fn initialize(
        &self,
        mut stopping: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), Error> {
        let discover = async {
            loop {
                match self.adapter().await {
                    Ok(Some(device)) => {
                        self.state()?.device = Some(device);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => self.log(
                        Level::Error,
                        format!("Error getting adapter type 2: {error}"),
                    ),
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok::<(), Error>(())
        };
        tokio::select! {
            biased;
            _ = stopping.wait_for(|stop| *stop) => return Ok(()),
            result = discover => result?,
        }
        let mut workers = JoinSet::new();
        let scanner = self.clone();
        workers.spawn_local(async move { scanner.scan().await });
        let monitor = self.clone();
        workers.spawn_local(async move { monitor.monitor().await });
        let setup = async {
            let result = async {
                let (paths,): (Vec<dbus::Path<'static>>,) = self
                    .main
                    .call(
                        Address {
                            path: bus::SETTINGS_PATH,
                            interface: bus::SETTINGS,
                        },
                        "ListConnections",
                        (),
                    )
                    .await?;
                let mut known = State::new(None);
                for path in paths {
                    let settings = self.connection_settings(&path).await?;
                    if settings.is_empty() {
                        self.warn(format!("Failed to get connection settings for {path}"));
                        continue;
                    }
                    if let Some(wifi) = settings.get("802-11-wireless") {
                        let ssid = wifi.get("ssid").ok_or(Error::Property("ssid"))?;
                        known.new_connection(
                            String::from_utf8_lossy(&bus::bytes(ssid, "ssid")?).into_owned(),
                            path.to_string(),
                        );
                    }
                }
                self.state()?.connections = known.connections;
                let hotspot = {
                    let state = self.state()?;
                    state
                        .connection(&state.tethering_ssid)
                        .is_none()
                        .then(|| state.tethering_ssid.clone())
                };
                if let Some(ssid) = hotspot {
                    match self
                        .main
                        .call::<_, (dbus::Path<'static>,)>(
                            Address {
                                path: bus::SETTINGS_PATH,
                                interface: bus::SETTINGS,
                            },
                            "AddConnection",
                            (settings::hotspot(&ssid),),
                        )
                        .await
                    {
                        Ok(_) | Err(Error::Reply(_)) => {}
                        Err(error) => return Err(error),
                    }
                }
                self.init_wifi_state().await?;
                let password = self.password().await?;
                self.state()?.snapshot.tethering_password = password;
                self.log(Level::Debug, "WifiManager initialized".into());
                Ok::<(), Error>(())
            }
            .await;
            if let Err(error) = result {
                self.log(
                    Level::Error,
                    format!("Wi-Fi initialization stopped: {error}"),
                );
            }
        };
        supervise(workers, setup, stopping).await
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::future::pending;
    use tokio::sync::{oneshot, watch};

    struct Dropped(Option<oneshot::Sender<()>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    async fn scanner(workers: &mut JoinSet<Result<(), Error>>) -> oneshot::Receiver<()> {
        let (started, readiness) = oneshot::channel();
        let (dropped, cleanup) = oneshot::channel();
        workers.spawn_local(async move {
            let _guard = Dropped(Some(dropped));
            started.send(()).unwrap();
            pending().await
        });
        readiness.await.unwrap();
        cleanup
    }
    #[tokio::test(flavor = "current_thread")]
    async fn monitor_failure_is_not_masked_by_live_scanner_or_pending_setup() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for pending_setup in [false, true] {
                    let mut workers = JoinSet::new();
                    let mut cleanup = scanner(&mut workers).await;
                    let (fail, failure) = oneshot::channel();
                    workers.spawn_local(async {
                        failure.await.unwrap();
                        Err(Error::Property("monitor fixture"))
                    });
                    let (_stop, stopping) = watch::channel(false);
                    let (started, setup_started) = oneshot::channel();
                    let setup = async move {
                        started.send(()).unwrap();
                        if pending_setup {
                            pending::<()>().await;
                        }
                    };
                    let supervisor = tokio::task::spawn_local(supervise(workers, setup, stopping));
                    setup_started.await.unwrap();
                    fail.send(()).unwrap();
                    let result = tokio::time::timeout(Duration::from_secs(1), supervisor).await;
                    assert!(matches!(
                        result,
                        Ok(Ok(Err(Error::Property("monitor fixture"))))
                    ));
                    cleanup
                        .try_recv()
                        .expect("scanner must be drained before owner returns");
                }
            })
            .await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn successful_exit_and_panic_are_unexpected_worker_failures() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for panic in [false, true] {
                    let mut workers = JoinSet::new();
                    let mut cleanup = scanner(&mut workers).await;
                    workers.spawn_local(async move {
                        assert!(!panic, "monitor fixture panic");
                        Ok(())
                    });
                    let (_stop, stopping) = watch::channel(false);
                    let result = tokio::time::timeout(
                        Duration::from_secs(1),
                        supervise(workers, async {}, stopping),
                    )
                    .await;
                    assert!(matches!(result, Ok(Err(_))));
                    cleanup.try_recv().unwrap();
                }
            })
            .await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn requested_stop_cancels_pending_setup_and_drains_healthy_workers() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for pending_setup in [false, true] {
                    let mut workers = JoinSet::new();
                    let mut first = scanner(&mut workers).await;
                    let mut second = scanner(&mut workers).await;
                    let (stop, stopping) = watch::channel(false);
                    let (started, setup_started) = oneshot::channel();
                    let setup = async move {
                        started.send(()).unwrap();
                        if pending_setup {
                            pending::<()>().await;
                        }
                    };
                    let supervisor = tokio::task::spawn_local(supervise(workers, setup, stopping));
                    setup_started.await.unwrap();
                    stop.send_replace(true);
                    let result = tokio::time::timeout(Duration::from_secs(1), supervisor).await;
                    assert!(matches!(result, Ok(Ok(Ok(())))));
                    first.try_recv().unwrap();
                    second.try_recv().unwrap();
                }
            })
            .await;
    }
}
