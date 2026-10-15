use super::{controller, events::Events, gpu, params::Reader, usb::Usb};
use openpilot_ui_application::scheduling::{self, Platform, Request, Scheduler};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

pub struct Native {
    board: bool,
    root: PathBuf,
    assets: PathBuf,
    usb: Usb,
    scheduler: Scheduler,
    scheduling: scheduling::Native,
    started: Instant,
    interrupt: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Native {
    pub fn set_interrupt(&mut self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.interrupt = Some(flag);
    }

    /// # Errors
    /// Returns executable-path or initial source scheduling failures, before Params construction.
    pub fn new(root: &Path) -> Result<Self, controller::Error> {
        let board = Path::new("/TICI").is_file();
        let mut scheduling = scheduling::Native { root: "/".into() };
        if board {
            scheduling.set_other(0)?;
        }
        let mut scheduler = Scheduler::new(board);
        scheduler.core = 4;
        let started = Instant::now();
        scheduler.update(
            Request {
                onroad: false,
                force: true,
                child_pid: None,
                now: 0.0,
            },
            &mut scheduling,
        )?;
        Ok(Self {
            board,
            root: root.to_owned(),
            assets: std::env::current_exe()?.with_file_name("usbgpu-assets"),
            usb: Usb::new(root, Path::new("/tmp/carrot_usbgpu_bus.lock")),
            scheduler,
            scheduling,
            started,
            interrupt: None,
        })
    }
}

impl controller::Platform for Native {
    fn interrupted(&self) -> bool {
        self.interrupt
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    }
    fn sleep(&self, duration: std::time::Duration) -> Result<(), controller::Error> {
        let end = Instant::now() + duration;
        loop {
            if self.interrupted() {
                return Err(controller::Error::Interrupted);
            }
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(std::time::Duration::from_millis(50)));
        }
    }

    fn board(&self) -> bool {
        self.board
    }

    fn offroad(&mut self) -> Result<(), controller::Error> {
        self.scheduler.update(
            Request {
                onroad: false,
                force: true,
                child_pid: None,
                now: self.started.elapsed().as_secs_f64(),
            },
            &mut self.scheduling,
        )?;
        Ok(())
    }

    fn startup(&mut self, params: &Reader) -> Result<(), controller::Error> {
        gpu::wait_for_usbgpu_startup_until(
            params,
            || {
                gpu::startup_expected(
                    &self.root,
                    &self.assets,
                    Path::new(openpilot_usbgpu::hardware::SYSFS),
                )
            },
            || self.interrupted(),
        )?;
        Ok(())
    }

    fn wifi_registered(&mut self) -> Result<bool, controller::Error> {
        openpilot_cluster_wifi::Registration::load(&openpilot_cluster_wifi::config_path())
            .map(|registration| registration.is_some())
            .map_err(|error| std::io::Error::other(error).into())
    }

    fn find(&mut self, product: u16) -> Option<u16> {
        self.usb.find(product)
    }

    fn dim(&mut self, product: u16, reason: &str) -> bool {
        self.usb.dim(product, reason)
    }

    fn events(&mut self) -> Events {
        Events::open()
    }
}
