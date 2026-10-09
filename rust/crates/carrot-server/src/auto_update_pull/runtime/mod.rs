//! Live lifecycle from services/auto_update.py, using the proved locked Attempt/Pull.
mod monitor;
pub mod notify;
mod reboot;
mod recipients;
mod recovered;

use super::{events::ErrorEvent, Effects, Failure, Policy, Pull, Update};
use crate::{
    config::Config, git_state::Store, git_status, repo_update, web_settings::WebSettings, Error,
    Value,
};
pub use monitor::{Inputs, ManagerMonitor};
use openpilot_params::Params;
pub use reboot::Reboot;
use std::{rc::Rc, sync::Arc, time::Duration};
use tokio::sync::watch;

pub struct Runtime {
    settings: WebSettings,
    params: Option<Params>,
    pull: Pull,
    recovery: Arc<repo_update::Recovery>,
    inputs: Inputs,
}

impl Runtime {
    pub fn new(
        config: &Config,
        params: Option<Params>,
        status: Arc<git_status::Service>,
    ) -> Arc<Self> {
        Self::with_inputs(config, params, status, Inputs::default())
    }

    pub fn with_inputs(
        config: &Config,
        params: Option<Params>,
        status: Arc<git_status::Service>,
        inputs: Inputs,
    ) -> Arc<Self> {
        let notify = Arc::new(notify::Notify::new(
            Arc::clone(&status),
            params.clone(),
            config.state.clone(),
        ));
        let alert_params = params.clone();
        let alert_repository = config.repository.clone();
        let repository = status.repository();
        let recovery = repo_update::Recovery::new(git_status::Repository {
            directory: repository.directory.clone(),
            lock: repository.lock.clone(),
            launcher: repository.launcher.clone(),
        });
        Arc::new(Self {
            settings: WebSettings::new(&config.state.join("web_settings.json"), &config.settings),
            params,
            pull: Pull::with_service(
                status,
                Arc::new(Store::new(config.state.clone())),
                Effects {
                    clock: Arc::clone(&inputs.wall),
                    alert: Arc::new(move |show, detail| {
                        recipients::alert(alert_params.as_ref(), &alert_repository, show, detail)
                    }),
                    notify: Arc::new(move |context| {
                        let notify = Arc::clone(&notify);
                        Box::pin(async move { notify.send(context).await })
                    }),
                },
            ),
            recovery,
            inputs,
        })
    }

    pub(crate) async fn clear_for_tools(
        &self,
        lock: Arc<std::fs::File>,
        stopped: watch::Receiver<bool>,
    ) -> Result<(), super::Failure> {
        self.pull
            .clear_recovered_git_ref_error(super::Notification {
                old_head: String::new(),
                lock,
                stopped,
            })
            .await
    }

    pub fn enabled(&self) -> bool {
        self.settings
            .read()
            .is_ok_and(|settings| settings.get("auto_update_git_pull").truth())
    }

    pub fn mode(&self) -> String {
        self.settings
            .read()
            .ok()
            .and_then(|settings| settings.get("auto_update_reboot").string().ok())
            .map(|mode| mode.trim().to_lowercase())
            .filter(|mode| matches!(mode.as_str(), "park" | "disengaged"))
            .unwrap_or_else(|| "off".into())
    }

    pub async fn run(self: &Arc<Self>, stopped: watch::Receiver<bool>) {
        self.run_with_timing(stopped, Duration::from_secs(60), Duration::ZERO)
            .await;
    }

    pub async fn run_with_timing(
        self: &Arc<Self>,
        mut stopped: watch::Receiver<bool>,
        interval: Duration,
        initial: Duration,
    ) {
        let manager = ManagerMonitor::new(self.inputs.clone());
        let monitor = Rc::clone(&manager);
        let observed_stop = stopped.clone();
        let observer =
            tokio::task::spawn_local(async move { monitor.observe(observed_stop).await });
        let sampled_manager = Rc::clone(&manager);
        let mut update = Update::new(
            self.pull.clone(),
            Arc::clone(&self.recovery),
            Policy {
                ready: Box::new(move || sampled_manager.ready()),
                monotonic: Arc::clone(&self.inputs.monotonic),
            },
        );
        if !sleep(initial, &mut stopped).await {
            let _ = observer.await;
            return;
        }
        let mut next_check = 0.;
        while !*stopped.borrow() {
            if self.enabled() && manager.ready() && (self.inputs.monotonic)() >= next_check {
                match update.run(stopped.clone()).await {
                    Ok((_, updated, head)) => {
                        next_check = (self.inputs.monotonic)() + interval.as_secs_f64();
                        let mode = self.mode();
                        if updated && mode != "off" {
                            if let Err(error) =
                                self.wait_reboot(&mode, &head, stopped.clone()).await
                            {
                                if *stopped.borrow() {
                                    break;
                                }
                                let recorded = self.pull.error(
                                    "reboot_monitor_failed",
                                    ErrorEvent {
                                        detail: error.to_string(),
                                        blocked: false,
                                        fields: Value::object([
                                            ("new_head", Value::text(&head)),
                                            ("target_head", Value::text(&head)),
                                            ("reboot_mode", Value::text(&mode)),
                                        ]),
                                    },
                                );
                                if let Err(error) = recorded {
                                    eprintln!("[auto_update] loop error: {error}");
                                }
                                println!("[auto_update] reboot monitor error: {error}");
                            }
                        }
                    }
                    Err(error) => {
                        if *stopped.borrow() {
                            break;
                        }
                        println!("[auto_update] loop error: {error}");
                    }
                }
            }
            if !sleep(Duration::from_secs(1), &mut stopped).await {
                break;
            }
        }
        if let Err(error) = observer.await {
            eprintln!("[auto_update] manager observer: {error}");
        }
    }

    pub async fn wait_reboot(
        &self,
        mode: &str,
        head: &str,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<(), Failure> {
        let mut sm = (self.inputs.messaging)(&["carState", "selfdriveState", "deviceState"])?;
        let Some(mut reboot) = Reboot::begin(&self.pull, mode, head)? else {
            return Ok(());
        };
        while !*stopped.borrow() {
            if reboot.select(&self.pull, &self.mode())? {
                return Ok(());
            }
            sm.update(Duration::ZERO)
                .map_err(|error| Error::Source(error.to_string()))?;
            let signals = monitor::signals(&sm)?;
            if reboot.sample(
                &self.pull,
                &signals.sample((self.inputs.monotonic)()),
                || {
                    let params = self
                        .params
                        .as_ref()
                        .ok_or_else(|| Error::Source("Params unavailable".into()))?;
                    recipients::param_result(params.put_bool("DoReboot", true))
                },
            )? {
                return Ok(());
            }
            if !sleep(Duration::from_millis(100), &mut stopped).await {
                break;
            }
        }
        Err(git_status::Failure::Cancelled.into())
    }
}

pub(crate) async fn stopping(stopped: &mut watch::Receiver<bool>) {
    while !*stopped.borrow_and_update() {
        if stopped.changed().await.is_err() {
            break;
        }
    }
}

async fn sleep(duration: Duration, stopped: &mut watch::Receiver<bool>) -> bool {
    if *stopped.borrow() {
        return false;
    }
    tokio::select! { () = tokio::time::sleep(duration) => true, () = stopping(stopped) => false }
}
