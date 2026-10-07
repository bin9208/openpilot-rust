use chrono::TimeZone;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_ui_application::{
    context::{Actions, Context, PrimeStatus, Translations},
    device::{Config, Device},
    params::store::Store,
    state::{messages::SERVICES, ModelStatus, UiState},
};
use openpilot_ui_framework::multilang::Multilang;
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc};
pub fn context(
    root: &Path,
    scene: &super::Scene,
    setup: (&super::Suite, &Path),
) -> Result<Context, Box<dyn std::error::Error>> {
    let (suite, output) = setup;
    let params = Rc::new(Store::new(openpilot_params::Params::open(
        &output.join("params"),
        "d",
    )?));
    let memory = Rc::new(Store::new(openpilot_params::Params::open(
        &output.join("memory"),
        "d",
    )?));
    for (key, value) in &scene.params {
        params.put(key, value)?;
    }
    let ui = UiState::new(params.as_ref(), 0., ModelStatus::default())?;
    let stamp = chrono::Local
        .with_ymd_and_hms(2026, 10, 1, 12, 34, 56)
        .single()
        .ok_or("fixture clock")?;
    Ok(Context {
        ui: Rc::new(RefCell::new(ui)),
        messages: Rc::new(RefCell::new(SubMaster::isolated(
            &SERVICES,
            Options::default(),
        )?)),
        device: Rc::new(RefCell::new(Device::new(Config {
            big: suite.config.big,
            pc: true,
            mici: !suite.config.big,
            target_fps: 20,
        }))),
        params,
        memory,
        translations: Translations::new(Multilang::new(
            &root.join("openpilot/selfdrive/ui/translations"),
            Some(&suite.language),
        )?),
        prime: Arc::new(PrimeStatus::new(-2)),
        api: openpilot_ui_application::services::Api::new(
            "http://127.0.0.1:9".into(),
            root.into(),
            output.join("persist"),
        ),
        poll_gate: Arc::default(),
        actions: Actions::default(),
        big: suite.config.big,
        pc: true,
        device_type: "pc".into(),
        now_monotonic: Rc::new(|| 0.),
        model_status: Rc::new(|| Ok(ModelStatus::default())),
        now_wall: Rc::new(move || stamp),
        source_root: root.into(),
        persist_root: output.join("persist"),
        callbacks: Rc::default(),
    })
}
