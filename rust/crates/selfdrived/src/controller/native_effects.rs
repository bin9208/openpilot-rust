use super::{
    effects::{Effects, Personality},
    param_conversion, Error,
};
use crate::{
    callbacks::{self, AlertParams, NativeParams},
    car_specific::CarSpecificParams,
};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
    Fields,
};
use openpilot_logmessaged::{JsonValue, JsonView};
use openpilot_params::Params;

pub struct NativeEffects<'a> {
    pub params: &'a Params,
    pub logger: &'a mut Logger,
}
impl NativeEffects<'_> {
    fn adapter(&mut self) -> NativeParams<'_> {
        NativeParams {
            params: self.params,
            logger: self.logger,
        }
    }
    fn raw(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        match self.params.get(key) {
            Ok(value) => Ok(value.filter(|value| !value.is_empty())),
            Err(openpilot_params::Error::Io(_)) => Ok(None),
            Err(error) => Err(callbacks::Error::Parameter(error).into()),
        }
    }
    fn cast_warning(
        &mut self,
        key: &str,
        bytes: &[u8],
        kind: &str,
        ordinal: u8,
    ) -> Result<(), Error> {
        self.logger.emit(log_site!(),Record::text(Level::Warning,format!("Failed to cast param {key} with value={} from type t=<ParamKeyType.{kind}: {ordinal}>",param_conversion::bytes_repr(bytes))))?;
        Ok(())
    }
    fn write_result(result: Result<(), openpilot_params::Error>) -> Result<(), Error> {
        match result {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(callbacks::Error::Parameter(error).into()),
        }
    }
}
impl AlertParams for NativeEffects<'_> {
    fn text(&mut self, key: &str) -> Result<Option<String>, callbacks::Error> {
        self.adapter().text(key)
    }
    fn boolean(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.adapter().boolean(key)
    }
    fn integer(&mut self, key: &str) -> Result<i32, callbacks::Error> {
        self.adapter().integer(key)
    }
}
impl CarSpecificParams for NativeEffects<'_> {
    type Error = callbacks::Error;
    fn get_bool(&mut self, key: &str) -> Result<bool, callbacks::Error> {
        self.boolean(key)
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), callbacks::Error> {
        self.adapter().put_bool(key, value)
    }
}
impl Effects for NativeEffects<'_> {
    fn presence(&mut self, key: &str) -> Result<bool, Error> {
        match key {
            "NNFFModelName" => Ok(self.text(key)?.is_some()),
            "Offroad_ExcessiveActuation" => {
                let Some(bytes) = self.raw(key)? else {
                    return Ok(false);
                };
                let json = param_conversion::json_text(&bytes)
                    .and_then(|text| JsonValue::parse(&text).ok());
                match json {
                    Some(value) => Ok(!matches!(value.view(), JsonView::Null)),
                    None => {
                        self.cast_warning(key, &bytes, "JSON", 5)?;
                        Ok(false)
                    }
                }
            }
            _ => Err(Error::Contract("unsupported typed presence key")),
        }
    }
    fn wide_camera(&mut self) -> Result<bool, Error> {
        let bytes = self.raw("UseWideCamera")?;
        Ok(match bytes {
            Some(bytes) => bytes == b"1",
            None => {
                openpilot_params::metadata("UseWideCamera").and_then(|info| info.default)
                    == Some("1")
            }
        })
    }
    fn personality(&mut self) -> Result<Personality, Error> {
        let Some(bytes) = self.raw("LongitudinalPersonality")? else {
            return Ok(Personality::standard());
        };
        match param_conversion::decimal(&bytes) {
            Some(value) => Ok(Personality(value)),
            None => {
                self.cast_warning("LongitudinalPersonality", &bytes, "INT", 2)?;
                Ok(Personality::standard())
            }
        }
    }
    fn remove(&mut self, key: &str) -> Result<(), Error> {
        Self::write_result(self.params.remove(key))
    }
    fn offroad(&mut self, key: &str, extra: Option<&str>) -> Result<(), Error> {
        let alerts = JsonValue::parse(include_str!(
            "../../../../../openpilot/selfdrive/selfdrived/alerts_offroad.json"
        ))?;
        let alert = alerts
            .get(key)
            .ok_or(Error::Contract("unknown offroad alert"))?;
        let JsonView::Object(fields) = alert.view() else {
            return Err(Error::Contract("offroad alert object"));
        };
        let mut parts = Vec::new();
        for (name, value) in fields {
            let name =
                JsonValue::codepoints(name.to_vec()).ok_or(Error::Contract("alert field name"))?;
            if !name.text_eq("extra") {
                parts.push(format!("{}: {}", name.to_json()?, value.to_json()?));
            }
        }
        parts.push(format!(
            "\"extra\": {}",
            JsonValue::text(extra.unwrap_or("")).to_json()?
        ));
        Self::write_result(
            self.params
                .put(key, format!("{{{}}}", parts.join(", ")).as_bytes()),
        )
    }
    fn event(&mut self, name: &str, fields: Fields) -> Result<(), Error> {
        self.logger
            .emit(log_site!(), Record::event(name, Vec::new(), fields)?)?;
        Ok(())
    }
    fn monotonic(&mut self) -> f64 {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
    }
    fn available_streams(&mut self) -> Result<Vec<openpilot_msgq::VisionStream>, Error> {
        Ok(openpilot_msgq::VisionClient::available_streams("camerad")?)
    }
    fn timestamp(&mut self) -> Result<u64, Error> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        u64::try_from(now.tv_sec)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1_000_000_000))
            .and_then(|seconds| seconds.checked_add(u64::try_from(now.tv_nsec).ok()?))
            .ok_or(Error::Contract("monotonic timestamp overflow"))
    }
}
