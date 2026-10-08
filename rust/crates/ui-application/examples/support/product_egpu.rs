use super::Scene;
use openpilot_ui_application::{
    context::Context, services::egpu::Backend, state::SlowParams, Error,
};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use openpilot_usbgpu::hardware::{self, Device, RuntimeStatus};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

#[derive(Default, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub devices: Vec<Device>,
    pub check_error: Option<String>,
    pub complete_at: Option<u32>,
}
pub struct Fixture {
    devices: Vec<Device>,
    result: Option<String>,
    complete_at: Option<u32>,
    release: AtomicBool,
    calls: AtomicUsize,
    removals: AtomicUsize,
}
impl Backend for Fixture {
    fn status(&self, state: &SlowParams) -> Result<String, Error> {
        Ok(hardware::status(
            &self.devices,
            RuntimeStatus {
                compiled: state.usbgpu_compiled,
                loading: state.usbgpu_loading,
                active: state.usbgpu_active,
                startup_failed: state.usbgpu_startup_failed,
                compile_pending: state.usbgpu_compile_pending,
            },
        ))
    }
    fn link(&self) -> Result<String, Error> {
        Ok(match hardware::single(&self.devices) {
            Some(device) if device.speed_mbps >= 1000 && device.speed_mbps % 1000 == 0 => {
                format!("{} Gbps", device.speed_mbps / 1000)
            }
            Some(device) => format!("{} Mbps", device.speed_mbps),
            None => "not connected".into(),
        })
    }
    fn check(&self, cancelled: &AtomicBool) -> Result<Option<String>, Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        while !self.release.load(Ordering::Acquire) && !cancelled.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(self.result.clone())
    }
    fn remove_compiled_manifest(&self) -> Result<(), Error> {
        self.removals.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl Fixture {
    pub fn new(scene: &Scene) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let options = scene.egpu.as_ref().ok_or("missing eGPU fixture")?;
        Ok(Arc::new(Self {
            devices: options.devices.clone(),
            result: options.check_error.clone(),
            complete_at: options.complete_at,
            release: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            removals: AtomicUsize::new(0),
        }))
    }
    pub fn create(
        context: &Context,
        canvas: &mut Canvas,
        scene: &Scene,
    ) -> Result<(WidgetHandle, Arc<Self>), Box<dyn std::error::Error>> {
        let fixture = Self::new(scene)?;
        let widget = if scene.config.big {
            WidgetHandle::new(openpilot_ui_application::settings::egpu::Egpu::new(
                context.clone(),
                fixture.clone(),
            )?)
        } else {
            WidgetHandle::new(
                openpilot_ui_application::mici::settings::egpu::Egpu::new(
                    context.clone(),
                    canvas,
                    fixture.clone(),
                )?
                .navigation(),
            )
        };
        Ok((widget, fixture))
    }
    pub fn before(&self, index: u32) {
        if self.complete_at == Some(index) {
            self.release.store(true, Ordering::Release);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"calls":self.calls.load(Ordering::Relaxed), "removals":self.removals.load(Ordering::Relaxed)})
    }
}
