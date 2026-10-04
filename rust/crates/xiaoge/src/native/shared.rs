use super::{platform, Error};
use crate::{
    config::Config,
    service::{State, Stream},
    settings::Settings,
};
use openpilot_params::Params;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    time::Duration,
};

pub struct Shared {
    pub state: Mutex<State>,
    pub snapshot: Condvar,
    pub vasm_operation: Mutex<()>,
    pub params: Option<Params>,
    pub config_path: PathBuf,
    pub running: AtomicBool,
}

impl Shared {
    pub fn state(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state
            .lock()
            .map_err(|_| Error::Contract("vision state poisoned"))
    }

    pub fn refresh(&self, force: bool) -> Result<(), Error> {
        let now = platform::monotonic()?;
        {
            let mut state = self.state()?;
            if !force && now - state.last_param_refresh < 1.0 {
                return Ok(());
            }
            state.last_param_refresh = now;
        }
        let settings = Settings::from_parameters(|name| match &self.params {
            Some(params) => match params.get(name) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Xiaoge default {name} after parameter read failure: {error}");
                    None
                }
            },
            None => None,
        });
        self.state()?.settings = settings;
        Ok(())
    }

    pub fn save_config(&self, value: &openpilot_logmessaged::JsonValue) -> Result<Config, Error> {
        let config = Config::normalize_json(value)?;
        let temporary = self.config_path.with_extension("tmp");
        let mut data = serde_json::to_vec(&config)?;
        data.push(b'\n');
        fs::write(&temporary, data)?;
        fs::rename(&temporary, &self.config_path)?;
        let _operation = self
            .vasm_operation
            .lock()
            .map_err(|_| Error::Contract("blindspot operation poisoned"))?;
        self.state()?.configure(config.clone())?;
        Ok(config)
    }

    pub fn clear_config(&self) -> Result<(), Error> {
        match fs::remove_file(&self.config_path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let _operation = self
            .vasm_operation
            .lock()
            .map_err(|_| Error::Contract("blindspot operation poisoned"))?;
        self.state()?.configure(Config::default())?;
        Ok(())
    }

    pub fn set_settings(&self, value: &openpilot_logmessaged::JsonValue) -> Result<(), Error> {
        let settings = self.state()?.settings.patch_json(value)?;
        if let Some(params) = &self.params {
            for (name, value) in settings.parameters()? {
                params
                    .put(name, value.to_string().as_bytes())
                    .map_err(|_| crate::Error::Invalid("could not persist ONNX settings"))?;
            }
        }
        self.state()?.settings = settings;
        Ok(())
    }

    pub fn snapshot(&self, stream: Stream) -> Result<Option<Arc<[u8]>>, Error> {
        let mut state = self.state()?;
        let camera = &mut state.cameras[stream.index()];
        camera.snapshot_request = camera
            .snapshot_request
            .checked_add(1)
            .ok_or(Error::Contract("snapshot request overflow"))?;
        let request = camera.snapshot_request;
        let (state, _) = self
            .snapshot
            .wait_timeout_while(state, Duration::from_secs(5), |state| {
                state.cameras[stream.index()].snapshot_response < request
                    && self.running.load(Ordering::Acquire)
            })
            .map_err(|_| Error::Contract("snapshot state poisoned"))?;
        let camera = &state.cameras[stream.index()];
        Ok(if camera.snapshot_response >= request {
            camera.jpeg.clone()
        } else {
            None
        })
    }

    pub fn status(&self) -> Result<Vec<u8>, Error> {
        self.refresh(false)?;
        Ok(self
            .state()?
            .status(platform::monotonic()?, platform::timestamp()?)?
            .to_json()?
            .into_bytes())
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        self.snapshot.notify_all();
    }
}
