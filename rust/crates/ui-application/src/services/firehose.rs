use super::{
    polling::{Gate, Poller},
    Api, Error,
};
use crate::{api::http::Session, params::typed};
use openpilot_logmessaged::{JsonValue, JsonView};
use openpilot_params::Params;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
pub const KEY: &str = "ApiCache_FirehoseStats";
pub fn initial(params: &Params) -> Result<JsonValue, Error> {
    let Some(bytes) = params.get(KEY)? else {
        return Ok(JsonValue::parse("0")?);
    };
    let count = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| JsonValue::parse(text).ok())
        .and_then(|stats| stats.get("firehose"));
    let integer = count.as_ref().and_then(|count| match count.view() {
        JsonView::Integer(value) => Some(value.to_owned()),
        JsonView::Bool(value) => Some(i32::from(value).to_string()),
        JsonView::Float(value) if value.is_finite() => Some(format!("{:.0}", value.trunc())),
        JsonView::Text(_) => count
            .to_utf8()
            .and_then(|value| typed::integer_text(&value)),
        _ => None,
    });
    Ok(JsonValue::parse(integer.as_deref().unwrap_or("0"))?)
}
pub struct Firehose {
    pub count: Arc<Mutex<JsonValue>>,
    pub params: Arc<Params>,
    pub api: Api,
    session: Session,
}
impl Firehose {
    pub fn new(params: Arc<Params>, api: Api) -> Result<Self, Error> {
        let count = Arc::new(Mutex::new(initial(&params)?));
        let session = api.session(None)?;
        Ok(Self {
            count,
            params,
            api,
            session,
        })
    }
    pub fn fetch(&mut self) -> Result<(), Error> {
        use crate::params::Read;
        let identity = self.params.string("DongleId")?;
        if identity.is_empty() || identity == openpilot_registration::UNREGISTERED_DONGLE_ID {
            return Ok(());
        }
        let token = self.api.token(&identity)?;
        let response = self.session.get(
            &format!("{}/v1/devices/{identity}/firehose_stats", self.api.host),
            Some(&token),
        )?;
        if response.status != 200 {
            return Ok(());
        }
        let data = JsonValue::parse(&response.text)?;
        if !data.is_object() {
            return Err(Error::Contract("firehose response must be an object"));
        }
        *self
            .count
            .lock()
            .map_err(|_| Error::Contract("firehose count poisoned"))? =
            data.get("firehose").unwrap_or(JsonValue::parse("0")?);
        let json = data
            .to_json()
            .map_err(|_| Error::Contract("firehose JSON serialization"))?;
        self.params.put(KEY, json.as_bytes())?;
        Ok(())
    }
    pub fn start(mut self, gate: Arc<Gate>) -> Result<Poller, std::io::Error> {
        Poller::start(gate, Duration::from_secs(30), move || {
            if let Err(error) = self.fetch() {
                openpilot_startup_ui::logging::emit(
                    openpilot_logging::record::Level::Error,
                    format!("Failed to fetch firehose stats: {error}"),
                );
            }
        })
    }
}
