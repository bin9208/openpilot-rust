use openpilot_hardware_control::{HardwareControl, LinuxPlatform, ProcessCommands};
use openpilot_ui_framework::Error;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
};
pub trait Hardware {
    fn brightness_busy(&self) -> bool;
    fn brightness(&mut self, value: i32) -> Result<(), Error>;
    fn display_power(&mut self, value: bool) -> Result<(), Error>;
    fn reboot(&mut self) -> Result<(), Error>;
}
pub struct Native {
    control: HardwareControl,
    platform: LinuxPlatform,
    brightness: Option<mpsc::Sender<i32>>,
    worker: Option<JoinHandle<()>>,
    busy: Arc<AtomicBool>,
}
impl Native {
    pub fn new(device: &str, launcher: PathBuf) -> Result<Self, Error> {
        let control = |device: &str| {
            if device == "pc" {
                HardwareControl::pc()
            } else {
                HardwareControl::board(device)
            }
        };
        let platform =
            |launcher| LinuxPlatform::new(std::path::Path::new("/"), ProcessCommands { launcher });
        let (sender, receiver) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        let task_busy = busy.clone();
        let task_control = control(device);
        let mut task_platform = platform(launcher.clone());
        let worker = std::thread::Builder::new()
            .name("ui-brightness".into())
            .spawn(move || {
                for value in receiver {
                    task_control.set_screen_brightness(&mut task_platform, f64::from(value));
                    task_busy.store(false, Ordering::Release);
                }
            })?;
        Ok(Self {
            control: control(device),
            platform: platform(launcher),
            brightness: Some(sender),
            worker: Some(worker),
            busy,
        })
    }
}
impl Hardware for Native {
    fn brightness_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
    fn brightness(&mut self, value: i32) -> Result<(), Error> {
        self.busy.store(true, Ordering::Release);
        self.brightness
            .as_ref()
            .ok_or(Error::Contract("brightness worker closed"))?
            .send(value)
            .map_err(|_| Error::Contract("brightness worker stopped"))
    }
    fn display_power(&mut self, value: bool) -> Result<(), Error> {
        self.control.set_display_power(&mut self.platform, value);
        Ok(())
    }
    fn reboot(&mut self) -> Result<(), Error> {
        self.control
            .reboot(&mut self.platform)
            .map_err(|error| Error::Io(std::io::Error::other(error)))
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.brightness.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Error,
                "UI brightness worker panicked".into(),
            );
        }
    }
}
