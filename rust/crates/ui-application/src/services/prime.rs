use super::{
    polling::{Gate, Poller},
    truthy, Api, Error,
};
use crate::api::http::Session;
use crate::{context::PrimeStatus, params::Read};
use openpilot_params::Params;
use std::{sync::Arc, time::Duration};
pub fn initial(params: &dyn Read, environment: Option<&str>) -> Result<i32, crate::Error> {
    let value = if let Some(value) = environment.filter(|value| !value.is_empty()) {
        crate::params::typed::integer_text(value)
    } else {
        params
            .bytes("PrimeType")?
            .as_deref()
            .and_then(crate::params::typed::integer_bytes)
    }
    .and_then(|value| value.parse::<i32>().ok());
    Ok(value.filter(|value| (-2..=5).contains(value)).unwrap_or(-2))
}
pub struct Prime {
    pub status: Arc<PrimeStatus>,
    pub params: Arc<Params>,
    pub api: Api,
    session: Session,
}
impl Prime {
    pub fn new(status: Arc<PrimeStatus>, params: Arc<Params>, api: Api) -> Result<Self, Error> {
        let session = api.session(Some(Duration::from_secs(10)))?;
        Ok(Self {
            status,
            params,
            api,
            session,
        })
    }
    pub fn fetch(&mut self) -> Result<(), Error> {
        let identity = self.params.string("DongleId")?;
        if identity.is_empty() || identity == openpilot_registration::UNREGISTERED_DONGLE_ID {
            return Ok(());
        }
        let token = self.api.token(&identity)?;
        let response = self.session.get(
            &format!("{}/v1.1/devices/{identity}", self.api.host),
            Some(&token),
        )?;
        if response.status != 200 {
            return Ok(());
        }
        use openpilot_logmessaged::{JsonValue, JsonView};
        let data = JsonValue::parse(&response.text)?;
        if !data.is_object() {
            return Err(Error::Contract("prime response must be an object"));
        }
        let paired = data.get("is_paired").as_ref().is_some_and(truthy);
        let value = if paired {
            match data.get("prime_type").as_ref().map(JsonValue::view) {
                None => 0,
                Some(JsonView::Bool(value)) => i32::from(value),
                Some(JsonView::Integer(value)) => value
                    .parse::<i32>()
                    .ok()
                    .filter(|value| (-2..=5).contains(value))
                    .ok_or(Error::Contract("invalid PrimeType"))?,
                Some(JsonView::Float(value)) => {
                    if !(-2.0..=5.0).contains(&value) || value.fract() != 0.0 {
                        return Err(Error::Contract("invalid PrimeType"));
                    }
                    use num_traits::ToPrimitive;
                    value.to_i32().ok_or(Error::Contract("invalid PrimeType"))?
                }
                Some(_) => return Err(Error::Contract("invalid PrimeType")),
            }
        } else {
            -1
        };
        if self.status.get() != value {
            self.status.set(value);
            self.params.put("PrimeType", value.to_string().as_bytes())?;
        }
        Ok(())
    }
    pub fn start(mut self, gate: Arc<Gate>) -> Result<Poller, std::io::Error> {
        Poller::start(gate, Duration::from_secs(5), move || {
            if let Err(error) = self.fetch() {
                openpilot_startup_ui::logging::emit(
                    openpilot_logging::record::Level::Error,
                    format!("Failed to fetch prime status: {error}"),
                );
            }
        })
    }
}
