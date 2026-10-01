use chrono::TimeZone;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_ui_application::{
    context::{Actions, Context, PrimeStatus, Translations},
    device::{Config as DeviceConfig, Device},
    params::store::Store,
    state::{messages::SERVICES, ModelStatus, UiState},
};
use openpilot_ui_framework::multilang::Multilang;
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc};
pub fn context(
    root: &Path,
    scene: &super::Scene,
    output: &Path,
) -> Result<Context, Box<dyn std::error::Error>> {
    let params = Rc::new(Store::new(openpilot_params::Params::open(
        &output.join("params"),
        "d",
    )?));
    let memory = Rc::new(Store::new(openpilot_params::Params::open(
        &output.join("memory"),
        "d",
    )?));
    for (key, value) in &scene.params {
        params.put(key, value.as_bytes())?;
    }
    if let Some(address) = &scene.address {
        memory.put("NetworkAddress", address.as_bytes())?;
    }
    let ui = UiState::new(params.as_ref(), 0.0, ModelStatus::default())?;
    let device = Device::new(DeviceConfig {
        big: scene.config.big,
        pc: true,
        mici: !scene.config.big,
        target_fps: 20,
    });
    let stamp = chrono::Local
        .with_ymd_and_hms(2026, 10, 1, 12, 34, 56)
        .single()
        .ok_or("fixture timestamp invalid")?;
    Ok(Context {
        ui: Rc::new(RefCell::new(ui)),
        messages: Rc::new(RefCell::new(SubMaster::isolated(
            &SERVICES,
            Options::default(),
        )?)),
        device: Rc::new(RefCell::new(device)),
        params,
        memory,
        translations: Translations::new(Multilang::new(
            &root.join("openpilot/selfdrive/ui/translations"),
            Some(&scene.language),
        )?),
        prime: Arc::new(PrimeStatus::new(scene.prime)),
        actions: Actions::default(),
        big: scene.config.big,
        pc: true,
        device_type: "pc".into(),
        now_wall: Rc::new(move || stamp),
        source_root: root.into(),
        persist_root: output.join("persist"),
        callbacks: Rc::default(),
    })
}
