use super::{
    mici,
    monitor::Monitor,
    tici,
    view::{Action, Shared},
    worker::{Layout, Options, Worker},
};
use crate::{
    context::{Action as ProductAction, Actions},
    mici::{settings::network::wifi::WifiContext, widgets::dialog},
};
use openpilot_ui_framework::{
    application::{Application, ApplicationConfig},
    network::WifiSession,
    widget::{NavigationRequest, WidgetHandle},
    Error as UiError,
};
use std::{path::Path, rc::Rc};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Ui(#[from] UiError),
    #[error(transparent)]
    Worker(#[from] super::worker::Error),
    #[error(transparent)]
    Monitor(#[from] super::monitor::Error),
    #[error(transparent)]
    Agnos(#[from] openpilot_agnos::Error),
    #[error(transparent)]
    Wifi(#[from] openpilot_wifi::Error),
    #[error(transparent)]
    Hardware(#[from] openpilot_hardware_control::Error),
}
struct NativeWifi(openpilot_wifi::WifiManager);
fn scan_result(
    command: &openpilot_wifi::Command,
    result: Result<(), openpilot_wifi::Error>,
) -> Result<(), UiError> {
    match result {
        Err(openpilot_wifi::Error::Stopped)
            if matches!(command, openpilot_wifi::Command::SetActive(_)) =>
        {
            Ok(())
        }
        result => result.map_err(|e| UiError::Io(std::io::Error::other(e))),
    }
}
impl openpilot_ui_framework::network::WifiBackend for NativeWifi {
    fn snapshot(&self) -> Result<openpilot_wifi::Snapshot, UiError> {
        self.0
            .snapshot()
            .map_err(|e| UiError::Io(std::io::Error::other(e)))
    }
    fn drain_events(&self) -> Result<Vec<openpilot_wifi::Event>, UiError> {
        self.0
            .drain_events()
            .map_err(|e| UiError::Io(std::io::Error::other(e)))
    }
    fn send(&self, command: openpilot_wifi::Command) -> Result<(), UiError> {
        scan_result(&command, self.0.send(command.clone()))
    }
}
pub trait Effects {
    fn reboot(&mut self) -> Result<(), Error>;
}
pub struct NativeEffects {
    hardware: openpilot_hardware_control::HardwareControl,
    platform: openpilot_hardware_control::LinuxPlatform,
}
impl Effects for NativeEffects {
    fn reboot(&mut self) -> Result<(), Error> {
        Ok(self.hardware.reboot(&mut self.platform)?)
    }
}
pub enum Views {
    Tici(WidgetHandle),
    Mici(mici::Views),
}
pub struct Runtime {
    pub application: Application,
    pub worker: Worker,
    pub shared: Rc<Shared>,
    pub views: Views,
    pub session: WifiSession,
    monitor: Monitor,
    actions: Actions,
    effects: Box<dyn Effects>,
}
impl Runtime {
    pub fn new(
        config: ApplicationConfig,
        options: Options,
        session: WifiSession,
        urls: Vec<String>,
        effects: impl Effects + 'static,
    ) -> Result<Self, Error> {
        let pc = config.graphics.pc;
        let mut application = Application::new(config)?;
        let worker = Worker::new(options.clone());
        let shared = Shared::new(worker.snapshot()?);
        let actions = Actions::default();
        let views = match options.layout {
            Layout::Tici => {
                let view = WidgetHandle::new(tici::View::new(
                    shared.clone(),
                    openpilot_ui_framework::network::Context {
                        session: session.clone(),
                        translate: {
                            let translations = application.translations.clone();
                            Rc::new(move |text| translations.borrow().tr(text).to_owned())
                        },
                    },
                    &mut application.canvas,
                )?);
                application.push(view.clone());
                Views::Tici(view)
            }
            Layout::Mici => {
                let context = WifiContext {
                    actions: actions.clone(),
                    pc,
                    now_monotonic: Rc::new(openpilot_startup_ui::diagnostics::monotonic_now),
                };
                let views = mici::Views::new(
                    shared.clone(),
                    context,
                    session.clone(),
                    &mut application.canvas,
                )?;

                application.push(views.root.clone());
                application.push(views.progress.clone());
                Views::Mici(views)
            }
        };
        let monitor = Monitor::start(urls, options.launcher)?;
        let mut runtime = Self {
            application,
            worker,
            shared,
            views,
            session,
            monitor,
            actions,
            effects: Box::new(effects),
        };
        runtime.worker.start()?;
        Ok(runtime)
    }
    pub fn native(root: &Path, updater: &Path, manifest: &Path) -> Result<Self, Error> {
        let config = ApplicationConfig::for_runtime(root, "System Update")?;
        let pc = config.graphics.pc;
        let layout = if config.graphics.large_viewport {
            Layout::Tici
        } else {
            Layout::Mici
        };
        let device = openpilot_hardware_info::for_runtime()
            .get_device_type()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        if layout == Layout::Mici {
            super::scheduling::configure(
                &mut super::scheduling::Native,
                pc,
                matches!(device.as_str(), "tici" | "tizi"),
            )?;
        }
        let launcher = std::env::current_exe()?.with_file_name("openpilot-process-child");
        let urls = openpilot_agnos::manifest::download_urls(manifest)?;
        let backend = openpilot_wifi::WifiManager::start(openpilot_wifi::Config {
            launcher: launcher.clone(),
            ..Default::default()
        })?;

        let session = WifiSession::new(NativeWifi(backend))?;
        let hardware = if pc {
            openpilot_hardware_control::HardwareControl::pc()
        } else {
            openpilot_hardware_control::HardwareControl::board(&device)
        };
        let effects = NativeEffects {
            hardware,
            platform: openpilot_hardware_control::LinuxPlatform::new(
                Path::new("/"),
                openpilot_hardware_control::ProcessCommands {
                    launcher: launcher.clone(),
                },
            ),
        };
        Self::new(
            config,
            Options {
                updater: updater.into(),
                manifest: manifest.into(),
                launcher,
                cwd: std::env::current_dir()?,
                layout,
            },
            session,
            urls,
            effects,
        )
    }
    pub fn render(&mut self) -> Result<bool, Error> {
        let state = self.worker.snapshot()?;
        *self.shared.state.borrow_mut() = state.clone();
        if self.worker.take_reboot()? {
            self.effects.reboot()?;
        }
        self.session.process()?;
        let Self {
            application,
            worker,
            shared,
            views,
            monitor,
            session,
            actions,
            ..
        } = self;
        let result = application.render_cycle(
            |frame, stack, canvas| {
                if let Views::Mici(views) = views {
                    views.failure(&state, frame)?;
                    if views.network_tick(frame, stack, session, monitor.connected())? {
                        monitor.invalidate();
                    }
                    views.process_callbacks()?;
                }
                dispatch_dialogs(actions, canvas, frame.navigation)?;
                loop {
                    let action = shared.actions.borrow_mut().pop_front();
                    let Some(action) = action else { break };
                    match action {
                        Action::Retry => {
                            if let Views::Mici(views) = views {
                                let shared = shared.clone();
                                frame.navigation.push(NavigationRequest::PopTo {
                                    target: views.root.clone(),
                                    instant: false,
                                    callback: Some(Box::new(move || shared.action(Action::Start))),
                                });
                            } else {
                                worker
                                    .start()
                                    .map_err(|e| UiError::Io(std::io::Error::other(e)))?;
                            }
                        }
                        Action::Start => {
                            if worker
                                .start()
                                .map_err(|e| UiError::Io(std::io::Error::other(e)))?
                            {
                                if let Views::Mici(views) = views {
                                    frame
                                        .navigation
                                        .push(NavigationRequest::Push(views.progress.clone()));
                                }
                            }
                        }
                        Action::Network => {
                            if let Views::Mici(views) = views {
                                views.go(views.network.clone(), frame);
                            }
                        }
                        Action::Progress => {
                            if let Views::Mici(views) = views {
                                if worker
                                    .snapshot()
                                    .map_err(|e| UiError::Io(std::io::Error::other(e)))?
                                    .running
                                {
                                    views.go(views.progress.clone(), frame);
                                } else {
                                    shared.action(Action::Retry);
                                }
                            }
                        }
                        Action::Reboot => {
                            shared.action(Action::Reboot);
                            break;
                        }
                    }
                }
                Ok(())
            },
            |frame, canvas, _, _, _| dispatch_dialogs(actions, canvas, frame.navigation),
        )?;
        if self.shared.actions.borrow().front() == Some(&Action::Reboot) {
            self.shared.actions.borrow_mut().pop_front();
            self.effects.reboot()?;
        }
        Ok(result)
    }
    pub fn run(&mut self) -> Result<(), Error> {
        while !self.application.is_closed() {
            self.render()?;
        }
        self.close()
    }
    pub fn close(&mut self) -> Result<(), Error> {
        self.monitor.stop()?;
        self.worker.close()?;
        if self.worker.take_reboot()? {
            self.effects.reboot()?;
        }
        Ok(())
    }
}
pub fn dispatch_dialogs(
    actions: &Actions,
    canvas: &mut openpilot_ui_framework::canvas::Canvas,
    navigation: &openpilot_ui_framework::widget::NavigationQueue,
) -> Result<(), UiError> {
    while let Some(action) = actions.pop() {
        let widget = match action {
            ProductAction::MiciInput(options) => {
                WidgetHandle::new(dialog::InputDialog::create(canvas, options)?)
            }
            ProductAction::MiciConfirm(options) => {
                WidgetHandle::new(dialog::confirmation(canvas, options)?)
            }
            ProductAction::MiciAlert { title, description } => {
                WidgetHandle::new(dialog::information(canvas, &title, &description)?)
            }
            ProductAction::Failure(error) => WidgetHandle::new(dialog::information(
                canvas,
                "network error",
                &error.to_string(),
            )?),
            _ => {
                return Err(UiError::Contract(
                    "unexpected standalone updater Wi-Fi action",
                ))
            }
        };
        navigation.push(NavigationRequest::Push(widget));
    }
    Ok(())
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("updater close: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_bus_scan_still_allows_updater_but_connect_errors_remain_visible() {
        assert!(scan_result(
            &openpilot_wifi::Command::SetActive(true),
            Err(openpilot_wifi::Error::Stopped)
        )
        .is_ok());
        assert!(scan_result(
            &openpilot_wifi::Command::Activate("Owned".into()),
            Err(openpilot_wifi::Error::Stopped)
        )
        .is_err());
        println!(
            "source missing-D-Bus scan fallback: tolerated activation, retained connect failure"
        );
    }
}
