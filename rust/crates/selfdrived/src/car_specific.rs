//! Native policy port of openpilot/selfdrive/car/car_specific.py (CarSpecificEvents).
//! MockCarState's GPS selection is a separate runtime responsibility.
mod brands;
mod common;
mod input;
pub use common::CommonOptions;

use crate::callbacks::{AlertParams, NativeParams};
use crate::events::Catalog;
pub use input::{
    Brand, Button, ButtonType, CarControl, CarInputs, CarParams, CarState, CruiseState,
    GearShifter, InputError, NetworkLocation,
};
use openpilot_cereal::log_capnp::onroad_event::EventName;
use serde::Serialize;
use std::collections::VecDeque;

pub const HYUNDAI_PREV_BUTTON_SAMPLES: usize = 8;
pub const BLUETOOTH_CANCEL: i16 = -3;
pub const MAX_CTRL_SPEED: f64 = (145.0 + 4.0) * (1.0 / 3.6);
pub const VW_DEFAULT_MIN_STEER_SPEED: f64 = 0.4;

/// Ordered synchronous effects; reads/writes use the original Params bool semantics.
pub trait CarSpecificParams {
    type Error;
    fn get_bool(&mut self, key: &str) -> Result<bool, Self::Error>;
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error>;
}

impl CarSpecificParams for NativeParams<'_> {
    type Error = crate::callbacks::Error;

    fn get_bool(&mut self, key: &str) -> Result<bool, Self::Error> {
        self.boolean(key)
    }

    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error> {
        match self.params.put_bool(key, value) {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("CarSpecificEvents Params effect failed: {0}")]
    Parameter(E),
    #[error("CarSpecificEvents counter exhausted its native range")]
    CounterOverflow,
}

#[derive(Serialize)]
pub struct CarSpecificEvents {
    #[serde(rename = "CP")]
    cp: CarParams,
    steering_unpressed: u64,
    low_speed_alert: bool,
    no_steer_warning: bool,
    silent_steer_warning: u64,
    cruise_buttons: VecDeque<bool>,
    do_shutdown: bool,
    frame: u64,
    mute_door: bool,
    mute_seatbelt: bool,
    #[serde(rename = "vCruise_prev")]
    v_cruise_prev: f64,
    #[serde(rename = "carrotCruise_prev")]
    carrot_cruise_prev: i16,
    tesla_lkas_button_prev: bool,
}

impl CarSpecificEvents {
    pub fn new(cp: CarParams) -> Self {
        Self {
            cp,
            steering_unpressed: 0,
            low_speed_alert: false,
            no_steer_warning: false,
            silent_steer_warning: 1,
            cruise_buttons: VecDeque::with_capacity(HYUNDAI_PREV_BUTTON_SAMPLES),
            do_shutdown: false,
            frame: 0,
            mute_door: false,
            mute_seatbelt: false,
            v_cruise_prev: 250.0,
            carrot_cruise_prev: 0,
            tesla_lkas_button_prev: false,
        }
    }

    pub fn update<P: CarSpecificParams>(
        &mut self,
        input: &CarInputs,
        params: &mut P,
        catalog: &Catalog,
    ) -> Result<Vec<EventName>, Error<P::Error>> {
        self.frame = self.frame.checked_add(1).ok_or(Error::CounterOverflow)?;
        self.update_params(params)?;
        let mut events = self.brand_events(input, params, catalog)?;
        let cs = &input.current;
        match &self.cp.brand {
            Brand::Tesla => {
                let pressed = cs
                    .button_events
                    .iter()
                    .any(|button| button.kind == ButtonType::Lkas && button.pressed);
                if pressed
                    && !self.tesla_lkas_button_prev
                    && params
                        .get_bool("ExperimentalModeConfirmed")
                        .map_err(Error::Parameter)?
                {
                    let value = !params
                        .get_bool("ExperimentalMode")
                        .map_err(Error::Parameter)?;
                    params
                        .put_bool("ExperimentalMode", value)
                        .map_err(Error::Parameter)?;
                }
                self.tesla_lkas_button_prev = pressed;
            }
            Brand::Body
            | Brand::Mock
            | Brand::Ford
            | Brand::Nissan
            | Brand::Chrysler
            | Brand::Honda
            | Brand::Toyota
            | Brand::Gm
            | Brand::Volkswagen
            | Brand::Hyundai
            | Brand::Other(_) => {}
        }
        if input.control.enabled && self.v_cruise_prev == 0.0 && cs.v_cruise > 0.0 {
            events.push(EventName::AudioPrompt);
        }
        if self.carrot_cruise_prev != cs.carrot_cruise {
            events.push(EventName::AudioPrompt);
        }
        self.carrot_cruise_prev = cs.carrot_cruise;
        self.v_cruise_prev = cs.v_cruise;
        events.sort_by_key(|event| u16::from(*event));
        Ok(events)
    }

    pub fn update_params<P: CarSpecificParams>(
        &mut self,
        params: &mut P,
    ) -> Result<(), Error<P::Error>> {
        if self.frame.is_multiple_of(100) {
            self.mute_seatbelt = params.get_bool("MuteSeatbelt").map_err(Error::Parameter)?;
            self.mute_door = params.get_bool("MuteDoor").map_err(Error::Parameter)?;
        }
        Ok(())
    }
}
