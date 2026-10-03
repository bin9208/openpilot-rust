use super::{
    ButtonType, CarInputs, CarSpecificEvents, CarSpecificParams, Error, GearShifter,
    BLUETOOTH_CANCEL, MAX_CTRL_SPEED,
};
use crate::{events::Catalog, state::EventType};
use openpilot_cereal::log_capnp::onroad_event::EventName;

/// The source accepts extra_gears but never reads it; it imposes no extra gear gate.
pub struct CommonOptions<'a> {
    pub extra_gears: &'a [GearShifter],
    pub pcm_enable: bool,
    pub allow_enable: bool,
    pub allow_button_cancel: bool,
}

impl Default for CommonOptions<'_> {
    fn default() -> Self {
        Self {
            extra_gears: &[],
            pcm_enable: true,
            allow_enable: true,
            allow_button_cancel: true,
        }
    }
}

impl CarSpecificEvents {
    pub(super) fn common<P: CarSpecificParams>(
        &mut self,
        input: &CarInputs,
        options: CommonOptions<'_>,
        params: &mut P,
        catalog: &Catalog,
    ) -> Result<Vec<EventName>, Error<P::Error>> {
        let cs = &input.current;
        let prev = &input.previous;
        let mut events = Vec::new();
        let gates = [
            (cs.door_open && !self.mute_door, EventName::DoorOpen),
            (
                cs.seatbelt_unlatched && !self.mute_seatbelt,
                EventName::SeatbeltNotLatched,
            ),
            (cs.gear_shifter == GearShifter::Park, EventName::WrongGear),
            (
                cs.gear_shifter == GearShifter::Neutral,
                EventName::WrongGear,
            ),
            (
                cs.gear_shifter == GearShifter::Reverse,
                EventName::ReverseGear,
            ),
            (!cs.cruise_state.available, EventName::WrongCarMode),
            (cs.esp_disabled, EventName::EspDisabled),
            (cs.esp_active, EventName::EspActive),
            (cs.stock_fcw, EventName::StockFcw),
            (cs.stock_aeb, EventName::StockAeb),
            (cs.v_ego > MAX_CTRL_SPEED, EventName::SpeedTooHigh),
            (cs.cruise_state.non_adaptive, EventName::WrongCruiseMode),
            (
                cs.brake_hold_active && self.cp.openpilot_longitudinal_control,
                EventName::BrakeHold,
            ),
            (cs.parking_brake, EventName::ParkBrake),
            (cs.acc_faulted, EventName::AccFaulted),
            (cs.steering_pressed, EventName::SteerOverride),
            (
                cs.brake_pressed && cs.standstill,
                EventName::PreEnableStandstill,
            ),
            (cs.gas_pressed, EventName::GasPressedOverride),
            (cs.vehicle_sensors_invalid, EventName::VehicleSensorsInvalid),
            (cs.invalid_lkas_setting, EventName::InvalidLkasSetting),
            (cs.low_speed_alert, EventName::BelowSteerSpeed),
            (cs.button_enable, EventName::ButtonEnable),
            (
                self.cp.pcm_cruise
                    && cs.activate_cruise == BLUETOOTH_CANCEL
                    && prev.activate_cruise != BLUETOOTH_CANCEL,
                EventName::ButtonCancel,
            ),
        ];
        events.extend(
            gates
                .into_iter()
                .filter_map(|(gate, event)| gate.then_some(event)),
        );
        for button in &cs.button_events {
            if button.kind == ButtonType::Cancel
                && (options.allow_button_cancel || !self.cp.pcm_cruise)
            {
                events.push(EventName::ButtonCancel);
                if cs.gear_shifter == GearShifter::Park && !self.do_shutdown {
                    self.do_shutdown = true;
                    params
                        .put_bool("DoShutdown", true)
                        .map_err(Error::Parameter)?;
                }
            }
        }
        self.steering_unpressed = if cs.steering_pressed {
            0
        } else {
            self.steering_unpressed
                .checked_add(1)
                .ok_or(Error::CounterOverflow)?
        };
        if cs.steer_fault_temporary {
            if cs.steering_pressed && (!prev.steer_fault_temporary || self.no_steer_warning) {
                self.no_steer_warning = true;
            } else {
                self.no_steer_warning = false;
                if self.silent_steer_warning > 0 || cs.standstill || self.steering_unpressed < 150 {
                    self.silent_steer_warning = self
                        .silent_steer_warning
                        .checked_add(1)
                        .ok_or(Error::CounterOverflow)?;
                    if self.silent_steer_warning > 20 {
                        events.push(EventName::SteerTempUnavailableSilent);
                    }
                } else {
                    events.push(EventName::SteerTempUnavailable);
                }
            }
        } else {
            self.no_steer_warning = false;
            self.silent_steer_warning = 0;
        }
        if cs.steer_fault_permanent {
            events.push(EventName::SteerUnavailable);
        }
        if options.pcm_enable {
            if cs.cruise_state.enabled && !prev.cruise_state.enabled && options.allow_enable {
                events.push(EventName::PcmEnable);
            } else if !cs.cruise_state.enabled {
                events.push(EventName::PcmDisable);
            }
        }
        if !self.cp.pcm_cruise {
            if cs.activate_cruise > 0 && prev.activate_cruise <= 0 {
                let no_entry = events.iter().any(|event| {
                    catalog.get(*event).is_some_and(|definition| {
                        definition
                            .definitions
                            .iter()
                            .any(|item| item.category == EventType::NoEntry)
                    })
                });
                if !no_entry {
                    events.push(EventName::ButtonEnable);
                }
            } else if cs.activate_cruise < 0 && prev.activate_cruise >= 0 {
                events.push(EventName::ButtonCancel);
            }
            if cs.soft_hold_active > 0 {
                events.push(EventName::SoftHold);
            }
        }
        Ok(events)
    }

    /// Direct source-compatible common policy, including independently callable options.
    pub fn create_common_events<P: CarSpecificParams>(
        &mut self,
        input: &CarInputs,
        params: &mut P,
        context: (&Catalog, CommonOptions<'_>),
    ) -> Result<Vec<EventName>, Error<P::Error>> {
        let mut events = self.common(input, context.1, params, context.0)?;
        events.sort_by_key(|event| u16::from(*event));
        Ok(events)
    }
}
