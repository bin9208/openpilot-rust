//! Project registration policy; hardware discovery and the visual spinner are external interfaces.
use jsonwebtoken::Algorithm;
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
};
use openpilot_logmessaged::{JsonValue, JsonView};
use openpilot_params::Params;
use openpilot_uploader::http::SigningKey;
use std::{
    fs::File,
    io::Read,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod api;
pub use api::{api_get, Response};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    TypedParams(#[from] openpilot_params_typed::Error),
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
    #[error(transparent)]
    Signing(#[from] openpilot_uploader::TransferError),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error(transparent)]
    Http(#[from] ureq::Error),
    #[error(transparent)]
    Request(#[from] ureq::http::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Clock(#[from] std::time::SystemTimeError),
    #[error("{0}")]
    Contract(&'static str),
    #[error("hardware: {0}")]
    Hardware(String),
    #[error("spinner: {0}")]
    Spinner(String),
    #[error("DongleId requires a UTF-8 STRING value")]
    IdentityType,
}

pub struct KeyPair {
    pub algorithm: Algorithm,
    pub private: String,
    pub public: String,
}

pub fn get_key_pair(persist: &Path) -> Result<Option<KeyPair>, Error> {
    for (name, algorithm) in [("id_rsa", Algorithm::RS256), ("id_ecdsa", Algorithm::ES256)] {
        let private_path = persist.join("comma").join(name);
        let public_path = private_path.with_extension("pub");
        if private_path.is_file() && public_path.is_file() {
            let mut private = File::open(private_path)?;
            let mut public = File::open(public_path)?;
            return Ok(Some(KeyPair {
                algorithm,
                private: read_text(&mut private)?,
                public: read_text(&mut public)?,
            }));
        }
    }
    Ok(None)
}
fn read_text(file: &mut File) -> Result<String, Error> {
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

pub const UNREGISTERED_DONGLE_ID: &str = "UnregisteredDevice";

pub trait Hardware {
    fn serial(&mut self) -> Result<String, Error>;
    fn imei(&mut self, slot: usize) -> Result<Option<String>, Error>;
}
pub trait Spinner {
    fn start(&mut self) -> Result<(), Error>;
    fn update(&mut self, text: &str) -> Result<(), Error>;
    fn close(&mut self) -> Result<(), Error>;
}
pub trait Clock {
    fn monotonic(&mut self) -> f64;
    fn unix_seconds(&mut self) -> Result<i64, Error>;
    fn sleep(&mut self, duration: Duration) -> Result<(), Error>;
}
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for SystemClock {
    fn monotonic(&mut self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
    fn unix_seconds(&mut self) -> Result<i64, Error> {
        i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
            .map_err(|_| Error::Contract("UTC seconds overflow"))
    }
    fn sleep(&mut self, duration: Duration) -> Result<(), Error> {
        std::thread::sleep(duration);
        Ok(())
    }
}

pub struct Registration<'a> {
    pub params: &'a Params,
    pub persist: &'a Path,
    pub api_host: &'a str,
    pub source_root: &'a Path,
    pub logger: &'a mut Logger,
}
impl Registration<'_> {
    pub fn is_registered_device(&mut self) -> Result<bool, Error> {
        Ok(
            openpilot_params_typed::get_string(self.params, "DongleId", self.logger)?
                .is_some_and(|id| id != UNREGISTERED_DONGLE_ID),
        )
    }
    pub fn register(
        &mut self,
        hardware: &mut dyn Hardware,
        clock: &mut dyn Clock,
        mut spinner: Option<&mut dyn Spinner>,
    ) -> Result<JsonValue, Error> {
        let mut identity =
            openpilot_params_typed::get_string(self.params, "DongleId", self.logger)?
                .map(|id| JsonValue::text(&id));
        let fallback = self.persist.join("comma/dongle_id");
        if identity.is_none() && fallback_is_file(&fallback)? {
            identity = Some(JsonValue::text(
                read_text(&mut File::open(fallback)?)?
                    .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')),
            ));
        }
        let pair = get_key_pair(self.persist)?;
        if pair.as_ref().is_none_or(|pair| pair.public.is_empty()) {
            identity = Some(JsonValue::text(UNREGISTERED_DONGLE_ID));
            self.logger.emit(
                log_site!(),
                Record::text(Level::Warning, "missing public key".into()),
            )?;
        } else if identity.is_none() {
            if let Some(spinner) = spinner.as_deref_mut() {
                spinner.start()?;
                spinner.update("registering device")?;
            }
            let serial = hardware.serial()?;
            let started = clock.monotonic();
            let mut imei1 = None;
            let mut imei2 = None;
            while imei1.is_none() && imei2.is_none() {
                match hardware
                    .imei(0)
                    .and_then(|first| hardware.imei(1).map(|second| (first, second)))
                {
                    Ok((first, second)) => {
                        imei1 = first;
                        imei2 = second;
                    }
                    Err(error) => {
                        self.exception("Error getting imei, trying again...", &error)?;
                        clock.sleep(Duration::from_secs(1))?;
                    }
                }
                if clock.monotonic() - started > 60.0 {
                    update_spinner(&mut spinner, &serial, &imei1, &imei2)?;
                }
            }
            let mut backoff = 0;
            let started = clock.monotonic();
            let pair = pair.ok_or(Error::Contract("registration key unavailable"))?;
            let key = SigningKey::from_pem(pair.algorithm, pair.private.into_bytes());
            loop {
                let result =
                    (|| {
                        let expiration = clock
                            .unix_seconds()?
                            .checked_add(3600)
                            .ok_or(Error::Contract("token expiration overflow"))?;
                        let token = key
                            .token_claims(&serde_json::json!({"register":true,"exp":expiration}))?;
                        self.logger.emit(
                            log_site!(),
                            Record::text(Level::Info, "getting pilotauth".into()),
                        )?;
                        let response = api_get(
                            self.api_host,
                            self.source_root,
                            "v2/pilotauth/",
                            &[
                                ("imei", imei1.as_deref()),
                                ("imei2", imei2.as_deref()),
                                ("serial", Some(&serial)),
                                ("public_key", Some(&pair.public)),
                                ("register_token", Some(&token)),
                            ],
                        )?;
                        if matches!(response.status, 402 | 403) {
                            self.logger.emit(
                                log_site!(),
                                Record::text(
                                    Level::Info,
                                    format!("Unable to register device, got {}", response.status),
                                ),
                            )?;
                            Ok(JsonValue::text(UNREGISTERED_DONGLE_ID))
                        } else {
                            JsonValue::parse(&response.text)?.get("dongle_id").ok_or(
                                Error::Contract("registration response has no dongle_id key"),
                            )
                        }
                    })();
                match result {
                    Ok(id) => {
                        identity = Some(id);
                        break;
                    }
                    Err(error) => {
                        self.exception("failed to authenticate", &error)?;
                        backoff = (backoff + 1).min(15);
                        clock.sleep(Duration::from_secs(backoff))?;
                    }
                }
                if clock.monotonic() - started > 60.0 {
                    update_spinner(&mut spinner, &serial, &imei1, &imei2)?;
                }
            }
            if let Some(spinner) = spinner {
                spinner.close()?;
            }
        }
        let identity = identity.ok_or(Error::Contract("registration identity unavailable"))?;
        if truthy(&identity) {
            let text = identity.to_utf8().ok_or(Error::IdentityType)?;
            // The Cython STRING conversion can raise, but it ignores Params::put's I/O status.
            match self.params.put("DongleId", text.as_bytes()) {
                Ok(()) | Err(openpilot_params::Error::Io(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(identity)
    }
    fn exception(&mut self, text: &str, error: &Error) -> Result<(), Error> {
        self.logger.emit(
            log_site!(),
            Record::text(Level::Error, text.into()).with_exception(format!("{error}\n")),
        )?;
        Ok(())
    }
}
fn update_spinner(
    spinner: &mut Option<&mut dyn Spinner>,
    serial: &str,
    first: &Option<String>,
    second: &Option<String>,
) -> Result<(), Error> {
    if let Some(spinner) = spinner.as_deref_mut() {
        spinner.update(&format!(
            "registering device - serial: {serial}, IMEI: ({}, {})",
            first.as_deref().unwrap_or("None"),
            second.as_deref().unwrap_or("None")
        ))?;
    }
    Ok(())
}
fn truthy(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}

fn fallback_is_file(path: &Path) -> Result<bool, Error> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 9 | 40)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
