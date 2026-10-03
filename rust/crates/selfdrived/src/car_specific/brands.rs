use super::common::CommonOptions;
use super::{
    Brand, ButtonType, CarInputs, CarSpecificEvents, CarSpecificParams, Error, GearShifter,
    NetworkLocation, HYUNDAI_PREV_BUTTON_SAMPLES, VW_DEFAULT_MIN_STEER_SPEED,
};
use crate::events::Catalog;
use openpilot_cereal::log_capnp::onroad_event::EventName;

impl CarSpecificEvents {
    pub(super) fn brand_events<P: CarSpecificParams>(
        &mut self,
        input: &CarInputs,
        params: &mut P,
        catalog: &Catalog,
    ) -> Result<Vec<EventName>, Error<P::Error>> {
        let mut options = CommonOptions::default();
        match &self.cp.brand {
            Brand::Body | Brand::Mock => return Ok(Vec::new()),
            Brand::Ford => options.extra_gears = &[GearShifter::Manumatic],
            Brand::Nissan => options.extra_gears = &[GearShifter::Brake],
            Brand::Chrysler => options.extra_gears = &[GearShifter::Low],
            Brand::Honda => options.pcm_enable = false,
            Brand::Gm => {
                options.extra_gears = &[
                    GearShifter::Sport,
                    GearShifter::Low,
                    GearShifter::Eco,
                    GearShifter::Manumatic,
                ];
                options.pcm_enable = self.cp.pcm_cruise;
            }
            Brand::Volkswagen => {
                options.extra_gears =
                    &[GearShifter::Eco, GearShifter::Sport, GearShifter::Manumatic];
                options.pcm_enable = self.cp.pcm_cruise;
            }
            Brand::Hyundai => {
                let enable = input.current.button_events.iter().any(|button| {
                    matches!(
                        button.kind,
                        ButtonType::AccelCruise
                            | ButtonType::DecelCruise
                            | ButtonType::Cancel
                            | ButtonType::MainCruise
                    )
                });
                if self.cruise_buttons.len() == HYUNDAI_PREV_BUTTON_SAMPLES {
                    self.cruise_buttons.pop_front();
                }
                self.cruise_buttons.push_back(enable);
                options.extra_gears = &[GearShifter::Sport, GearShifter::Manumatic];
                options.pcm_enable = self.cp.pcm_cruise;
                options.allow_button_cancel = false;
            }
            Brand::Toyota | Brand::Tesla | Brand::Other(_) => {}
        }
        let mut events = self.common(input, options, params, catalog)?;
        let cs = &input.current;
        let cc = &input.control;
        let cp = &self.cp;
        match &cp.brand {
            Brand::Chrysler => {
                if cp.min_steer_speed > 0.0 && cs.v_ego < cp.min_steer_speed + 0.5 {
                    self.low_speed_alert = true;
                } else if cs.v_ego > cp.min_steer_speed + 1.0 {
                    self.low_speed_alert = false;
                }
                if self.low_speed_alert {
                    events.push(EventName::BelowSteerSpeed);
                }
            }
            Brand::Honda => {
                if cp.pcm_cruise && cs.v_ego < cp.min_enable_speed {
                    events.push(EventName::BelowEngageSpeed);
                }
                if cp.pcm_cruise {
                    if cs.cruise_state.enabled && !input.previous.cruise_state.enabled {
                        events.push(EventName::PcmEnable);
                    } else if !cs.cruise_state.enabled
                        && (cc.accel >= 0.0 || !cp.openpilot_longitudinal_control)
                    {
                        events.push(if cs.v_ego < cp.min_enable_speed + 2.0 {
                            EventName::SpeedTooLow
                        } else {
                            EventName::CruiseDisabled
                        });
                    }
                }
                if cp.min_enable_speed > 0.0 && cs.v_ego < 0.001 {
                    events.push(EventName::ManualRestart);
                }
            }
            Brand::Toyota => {
                if cp.openpilot_longitudinal_control {
                    if cs.cruise_state.standstill && !cs.brake_pressed {
                        events.push(EventName::ResumeRequired);
                    }
                    if cs.v_ego < cp.min_enable_speed {
                        events.push(EventName::BelowEngageSpeed);
                        if cc.accel > 0.3 {
                            events.push(EventName::SpeedTooLow);
                        }
                        if cs.v_ego < 0.001 {
                            events.push(EventName::ManualRestart);
                        }
                    }
                }
            }
            Brand::Gm => {
                if cs.v_ego < cp.min_enable_speed
                    && !(cs.standstill
                        && cs.brake >= 20.0
                        && cp.network_location == NetworkLocation::FwdCamera)
                {
                    events.push(EventName::BelowEngageSpeed);
                }
                if cs.cruise_state.standstill {
                    events.push(EventName::ResumeRequired);
                }
                if cs.v_ego < cp.min_steer_speed {
                    events.push(EventName::BelowSteerSpeed);
                }
            }
            Brand::Volkswagen => {
                if cp.min_steer_speed - 1e-3 > VW_DEFAULT_MIN_STEER_SPEED
                    && cs.v_ego < cp.min_steer_speed + 1.0
                {
                    self.low_speed_alert = true;
                } else if cs.v_ego > cp.min_steer_speed + 2.0 {
                    self.low_speed_alert = false;
                }
                if self.low_speed_alert {
                    events.push(EventName::BelowSteerSpeed);
                }
                if cp.openpilot_longitudinal_control {
                    if cs.v_ego < cp.min_enable_speed + 0.5 {
                        events.push(EventName::BelowEngageSpeed);
                    }
                    if cc.enabled && cs.v_ego < cp.min_enable_speed {
                        events.push(EventName::SpeedTooLow);
                    }
                }
            }
            Brand::Hyundai => {
                if cs.v_ego < cp.min_steer_speed + 2.0 && cp.min_steer_speed > 10.0 {
                    self.low_speed_alert = true;
                }
                if cs.v_ego > cp.min_steer_speed + 4.0 {
                    self.low_speed_alert = false;
                }
                if self.low_speed_alert {
                    events.push(EventName::BelowSteerSpeed);
                }
            }
            Brand::Body
            | Brand::Mock
            | Brand::Ford
            | Brand::Nissan
            | Brand::Tesla
            | Brand::Other(_) => {}
        }
        Ok(events)
    }
}
