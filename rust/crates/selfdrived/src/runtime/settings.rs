use crate::{
    callbacks::AlertParams,
    controller::{
        effects::{Effects, Personality},
        native_effects::NativeEffects,
        Controller, Error,
    },
};
use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Clone)]
pub struct Settings {
    pub metric: bool,
    pub experimental: bool,
    pub personality: Personality,
}
pub type SharedSettings = Arc<Mutex<Settings>>;
impl Controller {
    pub(crate) fn refresh_runtime_settings(&mut self) -> Result<(), Error> {
        if let Some(settings) = &self.runtime_settings {
            let settings = settings
                .lock()
                .map_err(|_| Error::SettingsPoisoned)?
                .clone();
            self.is_metric = settings.metric;
            self.experimental_mode = settings.experimental;
            self.personality = settings.personality;
        }
        Ok(())
    }
}

pub fn worker(
    input: (Arc<Params>, Factory, bool),
    settings: SharedSettings,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<(), Error>> {
    thread::spawn(move || {
        let (params, factory, longitudinal) = input;
        let mut logger = factory.logger();
        let mut effects = NativeEffects {
            params: &params,
            logger: &mut logger,
        };
        while !stop.load(Ordering::Relaxed) {
            let metric = effects.boolean("IsMetric")?;
            settings.lock().map_err(|_| Error::SettingsPoisoned)?.metric = metric;
            let experimental = effects.boolean("ExperimentalMode")? && longitudinal;
            settings
                .lock()
                .map_err(|_| Error::SettingsPoisoned)?
                .experimental = experimental;
            let personality = effects.personality()?;
            settings
                .lock()
                .map_err(|_| Error::SettingsPoisoned)?
                .personality = personality;
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    })
}
