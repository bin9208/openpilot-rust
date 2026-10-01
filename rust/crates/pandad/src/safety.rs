use openpilot_cereal::car_capnp::car_params;
use serde::Serialize;

pub trait Effects {
    type Error;
    fn panda_count(&self) -> usize;
    fn boolean(&mut self, key: &str) -> Result<bool, Self::Error>;
    fn bytes(&mut self, key: &str) -> Result<Vec<u8>, Self::Error>;
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error>;
    fn set_safety(&mut self, panda: usize, model: u16, parameter: u16) -> Result<(), Self::Error>;
    fn set_alternative(&mut self, panda: usize, experience: u16) -> Result<(), Self::Error>;
    fn warning(&mut self, message: &str) -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("Panda safety effect failed: {0}")]
    Effect(E),
    #[error("invalid Panda safety CarParams: {0}")]
    Cereal(#[from] capnp::Error),
}

#[derive(Default, Serialize)]
pub struct Safety {
    initialized: bool,
    log_once: bool,
    safety_configured: bool,
    prev_obd_multiplexing: bool,
}

impl Safety {
    pub fn configure<E: Effects>(
        &mut self,
        onroad: bool,
        effects: &mut E,
    ) -> Result<(), Error<E::Error>> {
        if onroad && !self.safety_configured {
            self.update_multiplexing(effects)?;
            let bytes = self.fetch_car_params(effects)?;
            if !bytes.is_empty() {
                effects
                    .warning(&format!("got {} bytes CarParams", bytes.len()))
                    .map_err(Error::Effect)?;
                self.set_safety(&bytes, effects)?;
                self.safety_configured = true;
            }
        } else if !onroad {
            self.initialized = false;
            self.safety_configured = false;
            self.log_once = false;
        }
        Ok(())
    }

    fn update_multiplexing<E: Effects>(&mut self, effects: &mut E) -> Result<(), Error<E::Error>> {
        let elm = u16::from(car_params::SafetyModel::Elm327);
        if !self.initialized {
            self.prev_obd_multiplexing = false;
            for index in 0..effects.panda_count() {
                effects.set_safety(index, elm, 1).map_err(Error::Effect)?;
            }
            self.initialized = true;
        }
        let requested = effects
            .boolean("ObdMultiplexingEnabled")
            .map_err(Error::Effect)?;
        if requested != self.prev_obd_multiplexing {
            for index in 0..effects.panda_count() {
                let parameter = u16::from(index > 0 || !requested);
                effects
                    .set_safety(index, elm, parameter)
                    .map_err(Error::Effect)?;
            }
            self.prev_obd_multiplexing = requested;
            effects
                .put_bool("ObdMultiplexingChanged", true)
                .map_err(Error::Effect)?;
        }
        Ok(())
    }

    fn fetch_car_params<E: Effects>(
        &mut self,
        effects: &mut E,
    ) -> Result<Vec<u8>, Error<E::Error>> {
        if !effects
            .boolean("FirmwareQueryDone")
            .map_err(Error::Effect)?
        {
            return Ok(Vec::new());
        }
        if !self.log_once {
            effects
                .warning("Finished FW query, Waiting for params to set safety model")
                .map_err(Error::Effect)?;
            self.log_once = true;
        }
        if !effects.boolean("ControlsReady").map_err(Error::Effect)? {
            return Ok(Vec::new());
        }
        effects.bytes("CarParams").map_err(Error::Effect)
    }

    fn set_safety<E: Effects>(
        &self,
        mut bytes: &[u8],
        effects: &mut E,
    ) -> Result<(), Error<E::Error>> {
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut bytes,
            capnp::message::ReaderOptions::new(),
        )?;
        let params = message.get_root::<car_params::Reader<'_>>()?;
        let configs = params.get_safety_configs()?;
        let alternative = params.get_alternative_experience() as u16;
        for index in 0..effects.panda_count() {
            let (model, parameter) = if index < configs.len() as usize {
                let config = configs.get(index as u32);
                let model = match config.get_safety_model() {
                    Ok(model) => u16::from(model),
                    Err(capnp::NotInSchema(ordinal)) => ordinal,
                };
                (model, config.get_safety_param())
            } else {
                (u16::from(car_params::SafetyModel::Silent), 0)
            };
            effects.warning(&format!("Panda {index}: setting safety model: {model}, param: {parameter}, alternative experience: {alternative}"))
                .map_err(Error::Effect)?;
            effects
                .set_alternative(index, alternative)
                .map_err(Error::Effect)?;
            effects
                .set_safety(index, model, parameter)
                .map_err(Error::Effect)?;
        }
        Ok(())
    }
}
