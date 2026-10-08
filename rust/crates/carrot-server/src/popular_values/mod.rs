//! Original server/services/popular_values.py and features/setting_popular_values.py.
pub mod cache;
pub mod config;
mod defaults;
mod http;
mod online;
pub mod payload;
use crate::{
    http::Application, json_fields::set, params::Backend, settings::Catalog, Error, Value,
};
pub use http::{handle, matches};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::task::{JoinHandle, JoinSet};

struct Schedule {
    last_at: f64,
    tasks: JoinSet<()>,
}

pub struct Service {
    enabled: bool,
    client: Mutex<Option<Result<reqwest::Client, Error>>>,
    timestamp: Option<f64>,
    hostname: Option<String>,
    memory: Mutex<Option<Arc<Mutex<Value>>>>,
    schedule: Mutex<Schedule>,
}

fn timestamp() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_owned()
}

impl Service {
    pub fn new(session_enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled: session_enabled,
            client: Mutex::new(session_enabled.then(online::client)),
            timestamp: None,
            hostname: None,
            memory: Mutex::new(None),
            schedule: Mutex::new(Schedule {
                last_at: 0.,
                tasks: JoinSet::new(),
            }),
        })
    }

    pub fn for_test(session_enabled: bool, timestamp: f64, hostname: &str) -> Arc<Self> {
        let mut value = Self::new(session_enabled);
        if let Some(service) = Arc::get_mut(&mut value) {
            service.timestamp = Some(timestamp);
            service.hostname = Some(hostname.to_owned());
        }
        value
    }

    pub fn read(&self, params: &Backend, catalog: Option<&Catalog>) -> Result<Value, Error> {
        let (car_key, settings_hash) = context(params, catalog);
        let memory = self
            .memory
            .lock()
            .map_err(|_| Error::Source("popular cache lock poisoned".into()))?;
        match memory.as_ref() {
            Some(memory) => {
                let memory = memory
                    .lock()
                    .map_err(|_| Error::Source("popular cache lock poisoned".into()))?;
                cache::read(Some(&memory), &car_key, &settings_hash)
            }
            None => cache::read(None, &car_key, &settings_hash),
        }
    }

    pub fn read_application(&self, app: &Application) -> Result<Value, Error> {
        let params = app
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        self.read(&params, app.catalog(&params).ok().as_ref())
    }

    pub fn seed(&self, memory: Option<Value>) -> Result<(), Error> {
        *self
            .memory
            .lock()
            .map_err(|_| Error::Source("popular cache lock poisoned".into()))? =
            memory.map(|value| Arc::new(Mutex::new(value)));
        Ok(())
    }

    fn prepare(&self, app: &Application, upload: bool) -> Result<Option<online::Request>, Error> {
        let params = app
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        if !self.enabled || !params.has_params() {
            return Ok(None);
        }
        let catalog = app.catalog(&params).ok();
        let payload = if upload {
            let catalog = catalog
                .as_ref()
                .ok_or_else(|| Error::Source("settings unavailable".into()))?;
            let Some(payload) = payload::snapshot(
                &params,
                catalog,
                &self.hostname.clone().unwrap_or_else(hostname),
            )?
            else {
                return Ok(None);
            };
            Some(payload)
        } else {
            None
        };
        let mut url = config::endpoint(&params, !upload);
        if !upload {
            let (car_key, hash) = context(&params, catalog.as_ref());
            if car_key.is_empty() || url.is_empty() {
                return Ok(None);
            }
            let mut address =
                url::Url::parse(&url).map_err(|error| Error::Source(error.to_string()))?;
            address
                .query_pairs_mut()
                .append_pair("car_key_type", "CarSelected3")
                .append_pair("car_key", &car_key)
                .append_pair("settings_hash", &hash);
            url = address.into();
        }
        Ok(Some(online::Request {
            url,
            credentials: config::credentials(&params),
            timeout: config::env_float("CARROT_PARAM_VALUE_TIMEOUT_S", 4.).max(1.),
            payload,
        }))
    }

    async fn download(&self, app: &Application) -> Option<Arc<Mutex<Value>>> {
        let input = match self.prepare(app, false) {
            Ok(Some(input)) => input,
            Ok(None) | Err(_) => return None,
        };
        let client = self.client.lock().ok()?.as_ref()?.as_ref().ok()?.clone();
        let response = match online::request(&client, &input).await {
            Ok(response) => response,
            Err(_) => return None,
        };
        let data = Value::parse(&response.text).ok()?;
        if !(200..300).contains(&response.status) || !matches!(data, Value::Object(_)) {
            eprintln!(
                "[carrot_param_value] popular download failed status={}",
                response.status
            );
            return None;
        }
        let current_hash = {
            let params = app.params.lock().ok()?;
            context(&params, app.catalog(&params).ok().as_ref()).1
        };
        let saved = cache::store(
            &data,
            self.timestamp.unwrap_or_else(timestamp),
            &current_hash,
        )
        .ok()?;
        let saved = Arc::new(Mutex::new(saved));
        *self.memory.lock().ok()? = Some(Arc::clone(&saved));
        Some(saved)
    }

    pub async fn upload(&self, app: &Application) -> bool {
        let input = match self.prepare(app, true) {
            Ok(Some(input)) => input,
            Ok(None) | Err(_) => return false,
        };
        let attempts = config::env_int("CARROT_PARAM_VALUE_RETRY_COUNT", 5).max(1);
        let delay = config::env_float("CARROT_PARAM_VALUE_RETRY_DELAY_S", 15.).max(1.);
        let client = match self.client.lock() {
            Ok(client) => match client.as_ref().and_then(|client| client.as_ref().ok()) {
                Some(client) => client.clone(),
                None => return false,
            },
            Err(_) => return false,
        };
        for attempt in 1..=attempts {
            let response = online::request(&client, &input).await;
            match response {
                Ok(response) if (200..300).contains(&response.status) => return true,
                Ok(response) => eprintln!(
                    "[carrot_param_value] upload failed status={}",
                    response.status
                ),
                Err(error) => eprintln!("[carrot_param_value] upload worker failed: {error}"),
            }
            eprintln!("[carrot_param_value] upload failed attempt={attempt}/{attempts}");
            if attempt < attempts {
                if let Ok(duration) = std::time::Duration::try_from_secs_f64(delay) {
                    tokio::time::sleep(duration).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }
        }
        false
    }

    pub async fn refresh(&self, app: &Application, upload: bool) -> Result<Value, Error> {
        if !self.enabled {
            return self.read_application(app);
        }
        let uploaded = upload && self.upload(app).await;
        match self.download(app).await {
            Some(memory) => {
                let mut saved = memory
                    .lock()
                    .map_err(|_| Error::Source("popular cache lock poisoned".into()))?;
                set(&mut saved, "uploaded", Value::Bool(uploaded))?;
                Ok(saved.clone())
            }
            None => {
                let mut cache = self.read_application(app)?;
                set(&mut cache, "uploaded", Value::Bool(uploaded))?;
                Ok(cache)
            }
        }
    }

    pub fn schedule(
        self: &Arc<Self>,
        app: Arc<Application>,
        now: f64,
        min_interval: Option<f64>,
    ) -> Result<bool, Error> {
        let mut state = self
            .schedule
            .lock()
            .map_err(|_| Error::Source("popular schedule lock poisoned".into()))?;
        while let Some(result) = state.tasks.try_join_next() {
            if let Err(error) = result {
                eprintln!("popular refresh worker: {error}");
            }
        }
        let interval = min_interval
            .unwrap_or_else(|| config::env_float("CARROT_PARAM_VALUE_REFRESH_MIN_S", 60.));
        if !cache::should_schedule(
            self.enabled,
            now,
            state.last_at,
            interval,
            !state.tasks.is_empty(),
        ) {
            return Ok(false);
        }
        state.last_at = now;
        let service = Arc::clone(self);
        state.tasks.spawn(async move {
            service.download(&app).await;
        });
        Ok(true)
    }

    pub fn schedule_now(self: &Arc<Self>, app: Arc<Application>) -> Result<bool, Error> {
        self.schedule(app, self.timestamp.unwrap_or_else(timestamp), None)
    }

    pub fn start_upload(self: &Arc<Self>, app: Arc<Application>) -> Option<JoinHandle<()>> {
        if !self.enabled {
            return None;
        }
        let service = Arc::clone(self);
        Some(tokio::spawn(async move {
            if let Err(error) = service.refresh(&app, true).await {
                eprintln!("popular boot refresh: {error}");
            }
        }))
    }

    pub async fn wait_scheduled(&self) -> Result<(), Error> {
        let mut tasks = {
            let mut state = self
                .schedule
                .lock()
                .map_err(|_| Error::Source("popular schedule lock poisoned".into()))?;
            std::mem::take(&mut state.tasks)
        };
        while let Some(result) = tasks.join_next().await {
            result.map_err(|error| Error::Source(format!("popular refresh worker: {error}")))?;
        }
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let client = self
            .client
            .lock()
            .map_err(|_| Error::Source("popular client lock poisoned".into()))?
            .take();
        drop(client);
        {
            let mut state = self
                .schedule
                .lock()
                .map_err(|_| Error::Source("popular schedule lock poisoned".into()))?;
            state.tasks.abort_all();
        }
        let mut tasks = {
            let mut state = self
                .schedule
                .lock()
                .map_err(|_| Error::Source("popular schedule lock poisoned".into()))?;
            std::mem::take(&mut state.tasks)
        };
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                if !error.is_cancelled() {
                    return Err(Error::Source(format!("popular refresh worker: {error}")));
                }
            }
        }
        Ok(())
    }
}

fn context(params: &Backend, catalog: Option<&Catalog>) -> (String, String) {
    if !params.has_params() {
        return (String::new(), String::new());
    }
    (
        payload::param_text(params, "CarSelected3"),
        catalog
            .and_then(|catalog| payload::settings_hash(catalog).ok())
            .unwrap_or_default(),
    )
}
