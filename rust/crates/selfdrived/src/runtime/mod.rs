pub mod clock;
pub mod platform;
pub mod settings;
mod startup;

use crate::{
    callbacks,
    controller::{
        car_frame::CarFrame,
        config::{Config, Mode},
        effects::Effects,
        native_effects::NativeEffects,
        Controller, Error,
    },
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
};
use openpilot_params::Params;
use openpilot_ui_framework::multilang::Multilang;
use std::{
    io::Cursor,
    num::NonZeroU64,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub struct Runtime {
    pub controller: Controller,
    pub ratekeeper: clock::Ratekeeper,
    sockets: startup::Sockets,
    params: Arc<Params>,
    factory: Factory,
    logger: Logger,
}
impl Runtime {
    pub fn open(root: &Path, stop: &AtomicBool) -> Result<Option<Self>, Error> {
        let params = Arc::new(Params::for_runtime().map_err(callbacks::Error::Parameter)?);
        let factory = Factory::for_runtime()?;
        let mut logger = factory.logger();
        let language_value = crate::callbacks::AlertParams::text(
            &mut callbacks::NativeParams {
                params: &params,
                logger: &mut logger,
            },
            "LanguageSetting",
        )?;
        let language = Multilang::new(
            &root.join("openpilot/selfdrive/ui/translations"),
            language_value.as_deref(),
        )?;
        openpilot_runtime_version::get_build_metadata(root)?;
        logger.emit(
            log_site!(),
            Record::text(Level::Info, "selfdrived is waiting for CarParams".into()),
        )?;
        let bytes = loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            match params.get("CarParams") {
                Ok(Some(bytes)) if !bytes.is_empty() => break bytes,
                Ok(_) | Err(openpilot_params::Error::Io(_)) => {
                    thread::sleep(Duration::from_millis(100))
                }
                Err(error) => return Err(callbacks::Error::Parameter(error).into()),
            }
        };
        let reader = capnp::serialize::read_message(
            Cursor::new(&bytes),
            capnp::message::ReaderOptions {
                traversal_limit_in_words: Some(bytes.len() / 8),
                nesting_limit: 64,
            },
        )?;
        let cp = reader.get_root::<car_params::Reader<'_>>()?;
        logger.emit(
            log_site!(),
            Record::text(Level::Info, "selfdrived got CarParams".into()),
        )?;
        let config = Config::read(cp)?;
        let mode = Mode {
            replay: std::env::var_os("REPLAY").is_some(),
            simulation: std::env::var_os("SIMULATION").is_some(),
            testing_closet: std::env::var_os("TESTING_CLOSET").is_some(),
            device_type: openpilot_hardware_info::for_runtime().get_device_type()?,
            nvme_present: Path::new("/dev/nvme0").exists(),
            branch: openpilot_runtime_version::git::get_short_branch(Some(root))?,
        };
        let mut startup = startup::Startup::new(NativeEffects {
            params: &params,
            logger: &mut logger,
        });
        let controller = Controller::new(config, (mode, language), &mut startup)?;
        let sockets = startup.sockets()?;
        Ok(Some(Self {
            controller,
            ratekeeper: clock::Ratekeeper::default(),
            sockets,
            params,
            factory,
            logger,
        }))
    }

    pub fn run(&mut self, stop: Arc<AtomicBool>, frames: Option<NonZeroU64>) -> Result<(), Error> {
        let settings = Arc::new(Mutex::new(settings::Settings {
            metric: self.controller.is_metric,
            experimental: self.controller.experimental_mode,
            personality: self.controller.personality.clone(),
        }));
        self.controller.runtime_settings = Some(Arc::clone(&settings));
        let thread_stop = Arc::new(AtomicBool::new(false));
        let worker = settings::worker(
            (
                Arc::clone(&self.params),
                self.factory.clone(),
                self.controller.config.car.openpilot_longitudinal_control,
            ),
            settings,
            Arc::clone(&thread_stop),
        );
        let result = (|| {
            let mut remaining = frames.map(NonZeroU64::get);
            while !stop.load(Ordering::Relaxed) {
                if worker.is_finished() {
                    return Err(Error::Contract("Params thread stopped unexpectedly"));
                }
                let car = self.sockets.car_state.receive(Duration::from_millis(20))?;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let car = match car {
                    Some(bytes) => CarFrame::from_event(&bytes)?,
                    None => CarFrame::read(self.controller.previous.bytes.clone())?,
                };
                self.sockets.subscriber.update(Duration::ZERO)?;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut effects = NativeEffects {
                    params: &self.params,
                    logger: &mut self.logger,
                };
                self.controller.step(
                    (
                        &car,
                        &mut self.sockets.subscriber.state,
                        self.ratekeeper.lagging(),
                    ),
                    &mut effects,
                    &mut self.sockets.publisher,
                )?;
                self.ratekeeper.monitor_time(|| effects.monotonic());
                if let Some(remaining) = &mut remaining {
                    *remaining -= 1;
                    if *remaining == 0 {
                        break;
                    }
                }
            }
            Ok(())
        })();
        thread_stop.store(true, Ordering::Relaxed);
        let worker_result = worker
            .join()
            .map_err(|_| Error::Contract("Params thread panicked"))?;
        self.controller.runtime_settings = None;
        worker_result?;
        result
    }
}
