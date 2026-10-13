use openpilot_carrot_server::{
    auto_update_pull::{Effects, Pull},
    auto_update_runtime::Inputs,
    config::Config,
    git_state::{Store, Time},
    git_status::{Repository, Service},
    Error, Value,
};
use openpilot_params::Params;
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Deserialize)]
pub struct Input {
    pub mode: String,
    pub repository: PathBuf,
    pub source: PathBuf,
    pub state: PathBuf,
    pub params: PathBuf,
    pub lock: PathBuf,
    pub launcher: PathBuf,
    #[serde(default)]
    pub head: String,
    #[serde(default)]
    pub reboot_mode: String,
    #[serde(default)]
    pub steps: Vec<serde_json::Value>,
    #[serde(default)]
    pub failures: usize,
}

pub struct Fixture {
    pub config: Config,
    pub service: Arc<Service>,
    pub params: Params,
    pub inputs: Inputs,
    pub now: Arc<AtomicU64>,
    pub creates: Arc<AtomicUsize>,
    pub effects: Arc<Mutex<Vec<Value>>>,
    pub store: Arc<Store>,
}

impl Fixture {
    pub fn new(input: &Input) -> Result<Self, Error> {
        let mut config = Config::at(
            &input.source,
            &input.state,
            &input
                .source
                .join("openpilot/selfdrive/carrot_settings.json"),
        );
        config.state = input.state.clone();
        config.legacy_state = input.state.join("absent");
        let service = Service::with_clock(
            Repository {
                directory: input.repository.clone(),
                lock: input.lock.clone(),
                launcher: input.launcher.clone(),
            },
            || 1000.,
        );
        let params = Params::open(&input.params, "d")?;
        let now = Arc::new(AtomicU64::new(0_f64.to_bits()));
        let creates = Arc::new(AtomicUsize::new(0));
        let clock = Arc::clone(&now);
        let created = Arc::clone(&creates);
        let failures = input.failures;
        let inputs = Inputs {
            monotonic: Arc::new(move || f64::from_bits(clock.load(Ordering::SeqCst))),
            messaging: Arc::new(move |names| {
                if created.fetch_add(1, Ordering::SeqCst) < failures {
                    return Err(Error::Source("owned unavailable messaging".into()));
                }
                openpilot_messaging::runtime::SubMaster::isolated(
                    names,
                    openpilot_messaging::state::Options::default(),
                )
                .map_err(|error| Error::Source(error.to_string()))
            }),
            wall: Arc::new(|| Time {
                seconds: Value::Float(1700000000.75),
                nanoseconds: Value::integer(1700000000750000000_i64),
            }),
        };
        Ok(Self {
            config,
            service,
            params,
            inputs,
            now,
            creates,
            effects: Arc::new(Mutex::new(Vec::new())),
            store: Arc::new(Store::new(input.state.clone())),
        })
    }

    pub fn pull(&self) -> Pull {
        let effects = Arc::clone(&self.effects);
        Pull::with_service(
            Arc::clone(&self.service),
            Arc::clone(&self.store),
            Effects {
                clock: Arc::clone(&self.inputs.wall),
                alert: Arc::new(move |show, detail| {
                    effects
                        .lock()
                        .map_err(|error| Error::Source(error.to_string()))?
                        .push(Value::object([
                            ("show", Value::Bool(show)),
                            ("detail", if show { detail.clone() } else { Value::Null }),
                        ]));
                    Ok(())
                }),
                notify: Arc::new(|_| Box::pin(async { Ok(()) })),
            },
        )
    }

    pub fn held(&self, path: &std::path::Path) -> Result<Arc<File>, Error> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock()?;
        Ok(Arc::new(file))
    }

    pub fn result(&self) -> Result<Value, Error> {
        let effects = self
            .effects
            .lock()
            .map_err(|error| Error::Source(error.to_string()))?
            .clone();
        Ok(Value::object([
            ("state", self.store.read()),
            ("effects", Value::Array(effects)),
            (
                "creates",
                Value::integer(self.creates.load(Ordering::SeqCst)),
            ),
            (
                "reboot",
                Value::text(&String::from_utf8_lossy(
                    &self.params.get("DoReboot")?.unwrap_or_default(),
                )),
            ),
            (
                "alert",
                Value::text(&String::from_utf8_lossy(
                    &self
                        .params
                        .get("Offroad_CarrotAutoUpdateFailed")?
                        .unwrap_or_default(),
                )),
            ),
        ]))
    }
}
