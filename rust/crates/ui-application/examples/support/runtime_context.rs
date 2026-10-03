use openpilot_ui_application::{
    context::{Actions, Context, PrimeStatus, Translations},
    device::{Config as DeviceConfig, Device},
    params::store::Store,
    services::{self, egpu::Backend},
    settings::resources::{Network, Resources},
    state::{messages::SERVICES, ModelStatus, SlowParams, UiState},
};
use openpilot_ui_framework::{
    application::Application,
    multilang::Multilang,
    network::{WifiBackend, WifiSession},
    Error,
};
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
};
pub fn context(
    root: &Path,
    output: &Path,
    app: &Application,
) -> Result<(Context, Resources), Box<dyn std::error::Error>> {
    let params = Rc::new(Store::new(openpilot_params::Params::for_runtime()?));
    let memory = Rc::new(Store::new(openpilot_params::Params::for_runtime_at(
        &output.join("memory"),
    )?));
    let now = openpilot_startup_ui::diagnostics::monotonic_now;
    let context = Context {
        ui: Rc::new(RefCell::new(UiState::new(
            params.as_ref(),
            now(),
            ModelStatus::default(),
        )?)),
        messages: Rc::new(RefCell::new(
            openpilot_messaging::runtime::SubMaster::for_runtime(&SERVICES, Default::default())?,
        )),
        device: Rc::new(RefCell::new(Device::new(DeviceConfig {
            big: app.canvas.renderer.config.big,
            pc: true,
            mici: false,
            target_fps: 20,
        }))),
        params,
        memory,
        translations: Translations::new(Multilang::new(
            &root.join("openpilot/selfdrive/ui/translations"),
            Some("en"),
        )?),
        prime: Arc::new(PrimeStatus::new(0)),
        api: services::Api::new(
            "http://127.0.0.1:9".into(),
            root.into(),
            output.join("persist"),
        ),
        poll_gate: Arc::default(),
        actions: Actions::default(),
        big: app.canvas.renderer.config.big,
        pc: true,
        device_type: "pc".into(),
        now_monotonic: Rc::new(now),
        model_status: Rc::new(|| Ok(ModelStatus::default())),
        now_wall: Rc::new(chrono::Local::now),
        source_root: root.into(),
        persist_root: output.join("persist"),
        callbacks: Rc::default(),
    };
    context.sync_services();
    let resources = Resources {
        network: Network {
            session: WifiSession::new(Wifi)?,
            params: Rc::new(context.params.raw.as_ref().clone()),
        },
        ticks: app.ticks.clone(),
        egpu: Arc::new(Egpu),
    };
    Ok((context, resources))
}
struct Wifi;
impl WifiBackend for Wifi {
    fn snapshot(&self) -> Result<openpilot_wifi::Snapshot, Error> {
        Ok(Default::default())
    }
    fn drain_events(&self) -> Result<Vec<openpilot_wifi::Event>, Error> {
        Ok(Vec::new())
    }
    fn send(&self, _: openpilot_wifi::Command) -> Result<(), Error> {
        Ok(())
    }
}
struct Egpu;
impl Backend for Egpu {
    fn status(&self, _: &SlowParams) -> Result<String, openpilot_ui_application::Error> {
        Ok("not connected".into())
    }
    fn link(&self) -> Result<String, openpilot_ui_application::Error> {
        Ok("not connected".into())
    }
    fn check(&self, _: &AtomicBool) -> Result<Option<String>, openpilot_ui_application::Error> {
        Ok(None)
    }
    fn remove_compiled_manifest(&self) -> Result<(), openpilot_ui_application::Error> {
        Ok(())
    }
}
pub struct Hardware(pub Rc<RefCell<Vec<serde_json::Value>>>);
impl openpilot_ui_application::runtime::hardware::Hardware for Hardware {
    fn brightness_busy(&self) -> bool {
        false
    }
    fn brightness(&mut self, value: i32) -> Result<(), Error> {
        self.0
            .borrow_mut()
            .push(serde_json::json!({"brightness":value}));
        Ok(())
    }
    fn display_power(&mut self, value: bool) -> Result<(), Error> {
        self.0
            .borrow_mut()
            .push(serde_json::json!({"display":value}));
        Ok(())
    }
    fn reboot(&mut self) -> Result<(), Error> {
        self.0.borrow_mut().push(serde_json::json!({"reboot":true}));
        Ok(())
    }
}
