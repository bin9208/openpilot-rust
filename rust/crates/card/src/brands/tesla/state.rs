use super::{Error, FSD_14, HAS_VEHICLE_BUS, MISSING_DAS_SETTINGS};
use crate::{
    core::{Message, VehicleLog},
    state_helpers::{button_enable, SpeedFilter, SteeringPressed},
};
use num_traits::ToPrimitive;
use openpilot_can::{
    dbc::{Dbc, Definitions},
    parser::Parser,
    Packet,
};
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type as Button};
use serde::Serialize;
use std::{path::Path, sync::Arc};

#[derive(Default, Serialize)]
pub struct Extras {
    pub summon: bool,
    pub summon_prev: bool,
    pub cruise_enabled_prev: bool,
    pub fsd14_error_logged: bool,
    pub suspected_fsd14: bool,
    pub suspected_fsd14_clear_frames: u32,
    pub hands_on_level: f64,
    pub gas_pressed: bool,
    pub steering_disengage: bool,
    pub acc_cancel_last: i32,
    #[serde(rename = "das_accCancel")]
    pub das_acc_cancel: bool,
    pub das_acc_state_last: Option<f64>,
    pub das_acc_cancel_frames: u32,
    pub cruise_override: bool,
    pub coop_steering: bool,
    pub infotainment_3_finger_press: i32,
    pub tesla_speed_button_template: Option<[u8; 8]>,
    pub tesla_speed_button_template_nanos: u64,
    pub tesla_speed_limit_target: f64,
    pub tesla_speed_limit_target_nanos: u64,
    pub tesla_speed_limit_target_valid: bool,
    pub tesla_speed_units: String,
    pub tesla_manual_speed_adjustment_counter: u64,
    pub tesla_speed_auto_resume_gesture_counter: u64,
    pub _tesla_speed_resume_up_nanos: u64,
    pub _tesla_speed_resume_down_nanos: u64,
    pub _tesla_speed_resume_wait_idle: bool,
}
pub struct State {
    pub party: Parser,
    pub autopilot: Parser,
    pub vehicle: Option<Parser>,
    defs: Definitions,
    pub extras: Extras,
    pub out: Message,
    speed: SpeedFilter,
    pressed: SteeringPressed,
    flags: u32,
    steer_ratio: f64,
    pcm: bool,
    cluster_seen: bool,
    pub prints: Vec<String>,
    pub logs: Vec<VehicleLog>,
    pub soft_hold: i16,
}
const MPH_TO_MS: f64 = 1.609344 * (1. / 3.6);

fn f32_value(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
fn enum_value(
    defs: &Definitions,
    parser: &mut Parser,
    message: &str,
    signal: &str,
    now: u64,
) -> Result<Option<String>, Error> {
    let value = parser
        .signal_lazy(message, signal, now)?
        .to_i64()
        .ok_or(Error::Numeric)?;
    let address = parser.dbc.message(message)?.address;
    Ok(defs
        .get(&address)
        .and_then(|d| d.get(signal))
        .and_then(|d| d.get(&value))
        .cloned())
}
fn changes(current: i32, previous: i32, mapped: i32, button: Button) -> Vec<(Button, bool)> {
    if current == previous {
        return Vec::new();
    }
    [(previous, false), (current, true)]
        .into_iter()
        .filter(|(v, _)| *v != 0)
        .map(|(v, p)| (if v == mapped { button } else { Button::Unknown }, p))
        .collect()
}
impl State {
    fn drain_parsers(&mut self) {
        for p in [&mut self.party, &mut self.autopilot]
            .into_iter()
            .chain(self.vehicle.iter_mut())
        {
            self.logs.extend(
                std::mem::take(&mut p.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: crate::query::DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
    }
    pub fn new(
        root: &Path,
        flags: u32,
        steer_ratio: f64,
        pcm: bool,
        now: u64,
    ) -> Result<Self, Error> {
        let dbc = Arc::new(Dbc::load(&root.join("tesla_model3_party.dbc"))?);
        let defs = dbc.definitions()?;
        let vehicle = if flags & HAS_VEHICLE_BUS != 0 {
            let mut p = Parser::new(
                Arc::new(Dbc::load(&root.join("tesla_model3_vehicle.dbc"))?),
                1,
                now,
            );
            p.add("UI_status2", Some(2.), false, now)?;
            p.add("VCSEC_TPMSDisplay", Some(1.), false, now)?;
            Some(p)
        } else {
            None
        };
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            party: Parser::new(Arc::clone(&dbc), 0, now),
            autopilot: Parser::new(dbc, 2, now),
            vehicle,
            defs,
            extras: Extras {
                steering_disengage: true,
                coop_steering: true,
                tesla_speed_units: "KPH".into(),
                ..Extras::default()
            },
            out,
            speed: SpeedFilter::new()?,
            pressed: SteeringPressed::default(),
            flags,
            steer_ratio,
            pcm,
            cluster_seen: false,
            prints: Vec::new(),
            logs: Vec::new(),
            soft_hold: 0,
        })
    }
    pub fn observe_speed_wheel(&mut self, data: &[u8], now: u64) {
        let Ok(data) = <[u8; 8]>::try_from(data) else {
            return;
        };
        if data[0] & 3 != 1 {
            return;
        }
        let tick = data[3] & 0x3f;
        if tick == 0 {
            self.extras.tesla_speed_button_template = Some(data);
            self.extras.tesla_speed_button_template_nanos = now;
            self.extras._tesla_speed_resume_wait_idle = false;
            return;
        }
        if self.extras._tesla_speed_resume_wait_idle {
            return;
        }
        let direction = if tick & 0x20 == 0 { 1 } else { -1 };
        self.extras.tesla_manual_speed_adjustment_counter += 1;
        let opposite = if direction > 0 {
            self.extras._tesla_speed_resume_down_nanos
        } else {
            self.extras._tesla_speed_resume_up_nanos
        };
        if opposite != 0 && i128::from(now) - i128::from(opposite) <= 1_000_000_000 {
            self.extras.tesla_speed_auto_resume_gesture_counter += 1;
            self.extras._tesla_speed_resume_up_nanos = 0;
            self.extras._tesla_speed_resume_down_nanos = 0;
            self.extras._tesla_speed_resume_wait_idle = true;
        } else if direction > 0 {
            self.extras._tesla_speed_resume_up_nanos = now;
            self.extras._tesla_speed_resume_down_nanos = 0;
        } else {
            self.extras._tesla_speed_resume_down_nanos = now;
            self.extras._tesla_speed_resume_up_nanos = 0;
        }
    }
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        for p in packets {
            for f in &p.frames {
                if f.address == 0x3c2 && f.bus == 1 {
                    self.observe_speed_wheel(&f.data, p.mono_time);
                }
            }
        }
        self.party.update(packets)?;
        self.autopilot.update(packets)?;
        if let Some(v) = &mut self.vehicle {
            v.update(packets)?;
        }
        self.drain_parsers();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        let raw =
            f32_value(self.party.signal_lazy("DI_speed", "DI_vehicleSpeed", now)? * (1. / 3.6))?;
        ret.set_v_ego_raw(raw);
        let [v, a] = self.speed.update(f64::from(raw));
        ret.set_v_ego(f32_value(v)?);
        ret.set_a_ego(f32_value(a)?);
        let mut ws = ret.reborrow().init_wheel_speeds();
        ws.set_fl(f32_value(
            self.party
                .signal_lazy("ESP_wheelSpeeds", "ESP_wheelSpeedFrL", now)?
                * (1. / 3.6),
        )?);
        ws.set_fr(f32_value(
            self.party
                .signal_lazy("ESP_wheelSpeeds", "ESP_wheelSpeedFrR", now)?
                * (1. / 3.6),
        )?);
        ws.set_rl(f32_value(
            self.party
                .signal_lazy("ESP_wheelSpeeds", "ESP_wheelSpeedReL", now)?
                * (1. / 3.6),
        )?);
        ws.set_rr(f32_value(
            self.party
                .signal_lazy("ESP_wheelSpeeds", "ESP_wheelSpeedReR", now)?
                * (1. / 3.6),
        )?);
        let unit_raw = self.party.signal_lazy("DI_speed", "DI_uiSpeedUnits", now)?;
        let units = enum_value(
            &self.defs,
            &mut self.party,
            "DI_speed",
            "DI_uiSpeedUnits",
            now,
        )?;
        let ui = self.party.signal_lazy("DI_speed", "DI_uiSpeed", now)?;
        let ui_kph = if raw > 2. && ui > 2. {
            (ui * (1. / 3.6) - f64::from(raw)).abs() <= (ui * MPH_TO_MS - f64::from(raw)).abs()
        } else {
            match units.as_deref() {
                Some("DI_SPEED_KPH" | "KPH") => true,
                Some("DI_SPEED_MPH" | "MPH") => false,
                _ => unit_raw == 1.,
            }
        };
        ret.set_v_ego_cluster(f32_value(ui * if ui_kph { 1. / 3.6 } else { MPH_TO_MS })?);
        let pedal = self
            .party
            .signal_lazy("DI_systemStatus", "DI_accelPedalPos", now)?;
        ret.set_gas(f32_value(pedal / 100.)?);
        self.extras.gas_pressed = pedal > if self.extras.gas_pressed { 0.4 } else { 0.8 };
        ret.set_gas_pressed(self.extras.gas_pressed);
        ret.set_engine_rpm(f32_value(self.party.signal_lazy(
            "DI_torque",
            "DI_axleSpeed",
            now,
        )?)?);
        ret.set_brake_pressed(
            self.party
                .signal_lazy("ESP_status", "ESP_driverBrakeApply", now)?
                == 2.,
        );
        ret.set_brake_lights(self.party.signal_lazy("ESP_status", "ESP_brakeLamp", now)? == 1.);
        ret.set_regen_braking(
            self.party
                .signal_lazy("DI_systemStatus", "DI_regenLight", now)?
                != 0.,
        );
        ret.set_esp_disabled(
            self.party
                .signal_lazy("ESP_status", "ESP_espFaultLamp", now)?
                != 0.,
        );
        ret.set_esp_active(
            self.party
                .signal_lazy("ESP_status", "ESP_espModeActive", now)?
                != 0.,
        );
        self.extras.hands_on_level =
            self.party
                .signal_lazy("EPAS3S_sysStatus", "EPAS3S_handsOnLevel", now)?;
        ret.set_steering_angle_deg(f32_value(-self.party.signal_lazy(
            "EPAS3S_sysStatus",
            "EPAS3S_internalSAS",
            now,
        )?)?);
        ret.set_steering_rate_deg(f32_value(-self.autopilot.signal_lazy(
            "SCCM_steeringAngleSensor",
            "SCCM_steeringAngleSpeed",
            now,
        )?)?);
        let torque = f32_value(-self.party.signal_lazy(
            "EPAS3S_sysStatus",
            "EPAS3S_torsionBarTorque",
            now,
        )?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_torque_eps(f32_value(
            -self
                .party
                .signal_lazy("EPAS3S_sysStatus", "EPAS3S_steeringRackForce", now)?
                * 0.11
                / self.steer_ratio,
        )?);
        ret.set_steering_pressed(self.pressed.update(f64::from(torque).abs() > 1., 5));
        let eac = enum_value(
            &self.defs,
            &mut self.party,
            "EPAS3S_sysStatus",
            "EPAS3S_eacStatus",
            now,
        )?;
        ret.set_steer_fault_permanent(eac.as_deref() == Some("EAC_FAULT"));
        ret.set_steer_fault_temporary(eac.as_deref() == Some("EAC_INHIBITED"));
        let error = enum_value(
            &self.defs,
            &mut self.party,
            "EPAS3S_sysStatus",
            "EPAS3S_eacErrorCode",
            now,
        )?;
        self.extras.steering_disengage = self.extras.hands_on_level >= 3.
            || (eac.as_deref() == Some("EAC_INHIBITED")
                && error.as_deref() == Some("EAC_ERROR_HIGH_ANGLE_RATE_SAFETY"));
        let cruise = enum_value(
            &self.defs,
            &mut self.party,
            "DI_state",
            "DI_cruiseState",
            now,
        )?;
        let cruise_units = enum_value(
            &self.defs,
            &mut self.party,
            "DI_state",
            "DI_speedUnits",
            now,
        )?;
        let acc = self
            .autopilot
            .signal_lazy("DAS_control", "DAS_accState", now)?;
        if self
            .extras
            .das_acc_state_last
            .is_some_and(|a| a == 3. || a == 4.)
            && [0., 1., 2., 12., 13., 14., 15.].contains(&acc)
        {
            self.extras.das_acc_cancel_frames = 4;
        }
        self.extras.das_acc_state_last = Some(acc);
        self.extras.das_acc_cancel = self.extras.das_acc_cancel_frames > 0;
        if self.extras.das_acc_cancel_frames > 0 {
            self.extras.das_acc_cancel_frames -= 1;
        }
        let summon = enum_value(
            &self.defs,
            &mut self.party,
            "DI_state",
            "DI_autoparkState",
            now,
        )?;
        let enabled = cruise.as_deref().is_some_and(|c| {
            [
                "ENABLED",
                "STANDSTILL",
                "OVERRIDE",
                "PRE_FAULT",
                "PRE_CANCEL",
            ]
            .contains(&c)
        });
        self.extras.cruise_override = cruise.as_deref() == Some("OVERRIDE");
        let summon_now = summon
            .as_deref()
            .is_some_and(|s| ["ACTIVE", "COMPLETE", "SELFPARK_STARTED"].contains(&s));
        if summon_now && !self.extras.summon_prev && !self.extras.cruise_enabled_prev {
            self.extras.summon = true;
        }
        if !summon_now {
            self.extras.summon = false;
        }
        self.extras.summon_prev = summon_now;
        self.extras.cruise_enabled_prev = enabled;
        let enabled = enabled && !self.extras.summon;
        let cruise_kph = match cruise_units.as_deref() {
            Some("KPH" | "DI_SPEED_KPH") => true,
            Some("MPH" | "DI_SPEED_MPH") => false,
            _ => ui_kph,
        };
        self.extras.tesla_speed_units = if cruise_kph { "KPH" } else { "MPH" }.into();
        let cluster = f32_value(
            self.party.signal_lazy("DI_state", "DI_digitalSpeed", now)?
                * if cruise_kph { 1. / 3.6 } else { MPH_TO_MS },
        )?;
        let mut cr = ret.reborrow().init_cruise_state();
        cr.set_enabled(enabled);
        cr.set_speed_cluster(cluster);
        cr.set_speed(f32_value(f64::from(cluster).max(1e-3))?);
        cr.set_available(cruise.as_deref() == Some("STANDBY") || enabled);
        ret.set_standstill(
            self.party
                .signal_lazy("ESP_B", "ESP_vehicleStandstillSts", now)?
                == 1.,
        );
        ret.set_acc_faulted(cruise.as_deref() == Some("FAULT"));
        let cancel = i32::from(self.extras.das_acc_cancel);
        let mut buttons = changes(cancel, self.extras.acc_cancel_last, 1, Button::Cancel);
        self.extras.acc_cancel_last = cancel;
        let limit = self
            .autopilot
            .signal_lazy("DAS_status", "DAS_fusedSpeedLimit", now)?;
        let address = self.autopilot.dbc.message("DAS_status")?.address;
        let time = self
            .autopilot
            .states
            .get(&address)
            .and_then(|s| s.timestamps.back())
            .copied()
            .unwrap_or(0);
        if limit > 0. && limit <= 150. && time > 0 {
            self.extras.tesla_speed_limit_target =
                limit * if cruise_kph { 1. / 3.6 } else { MPH_TO_MS };
            ret.set_speed_limit(f32_value(self.extras.tesla_speed_limit_target * 3.6)?);
            self.extras.tesla_speed_limit_target_nanos = time;
            self.extras.tesla_speed_limit_target_valid = true;
        } else {
            self.extras.tesla_speed_limit_target = 0.;
            self.extras.tesla_speed_limit_target_nanos = 0;
            self.extras.tesla_speed_limit_target_valid = false;
        }
        ret.set_parking_brake(
            enum_value(
                &self.defs,
                &mut self.party,
                "DI_state",
                "DI_parkBrakeState",
                now,
            )?
            .as_deref()
                == Some("APPLIED"),
        );
        ret.set_brake_hold_active(
            enum_value(
                &self.defs,
                &mut self.party,
                "DI_state",
                "DI_vehicleHoldState",
                now,
            )?
            .as_deref()
                == Some("STANDSTILL"),
        );
        ret.set_gear_shifter(
            match enum_value(
                &self.defs,
                &mut self.party,
                "DI_systemStatus",
                "DI_gear",
                now,
            )?
            .as_deref()
            {
                Some("DI_GEAR_P") => car_state::GearShifter::Park,
                Some("DI_GEAR_R") => car_state::GearShifter::Reverse,
                Some("DI_GEAR_N") => car_state::GearShifter::Neutral,
                Some("DI_GEAR_D") => car_state::GearShifter::Drive,
                _ => car_state::GearShifter::Unknown,
            },
        );
        ret.set_door_open(self.party.signal_lazy("UI_warning", "anyDoorOpen", now)? == 1.);
        ret.set_left_blinker([1., 2.].contains(&self.party.signal_lazy(
            "UI_warning",
            "leftBlinkerBlinking",
            now,
        )?));
        ret.set_right_blinker([1., 2.].contains(&self.party.signal_lazy(
            "UI_warning",
            "rightBlinkerBlinking",
            now,
        )?));
        ret.set_generic_toggle(self.party.signal_lazy("UI_warning", "highBeam", now)? == 1.);
        ret.set_seatbelt_unlatched(
            self.party.signal_lazy("UI_warning", "buckleStatus", now)? != 1.,
        );
        ret.set_left_blindspot(
            self.autopilot
                .signal_lazy("DAS_status", "DAS_blindSpotRearLeft", now)?
                != 0.,
        );
        ret.set_right_blindspot(
            self.autopilot
                .signal_lazy("DAS_status", "DAS_blindSpotRearRight", now)?
                != 0.,
        );
        ret.set_stock_aeb(
            self.autopilot
                .signal_lazy("DAS_control", "DAS_aebEvent", now)?
                == 1.,
        );
        ret.set_stock_fcw(
            self.autopilot
                .signal_lazy("DAS_status", "DAS_forwardCollisionWarning", now)?
                != 0.,
        );
        if self.flags & MISSING_DAS_SETTINGS == 0 {
            ret.set_invalid_lkas_setting(
                self.autopilot
                    .signal_lazy("DAS_settings", "DAS_autosteerEnabled", now)?
                    != 0.,
            );
            let angle = self.autopilot.signal_lazy(
                "DAS_steeringControl",
                "DAS_steeringControlType",
                now,
            )? == 1.
                && eac.as_deref() != Some("EMERGENCY_LANE_KEEP");
            if !ret.reborrow_as_reader().get_invalid_lkas_setting()
                && angle
                && self.flags & FSD_14 == 0
            {
                self.extras.suspected_fsd14 = true;
                self.extras.suspected_fsd14_clear_frames = 0;
            }
            if self.extras.suspected_fsd14 {
                ret.set_invalid_lkas_setting(true);
                if !self.extras.fsd14_error_logged {
                    self.logs.push(VehicleLog {
                        level: crate::query::DiagnosticLevel::Error,
                        message: "FSD 14 detected, but FW not in FSD_14_FW set".into(),
                    });
                    self.extras.fsd14_error_logged = true;
                }
                if !angle {
                    self.extras.suspected_fsd14_clear_frames += 1;
                    if self.extras.suspected_fsd14_clear_frames >= 100 {
                        self.extras.suspected_fsd14 = false;
                        self.extras.suspected_fsd14_clear_frames = 0;
                    }
                } else {
                    self.extras.suspected_fsd14_clear_frames = 0;
                }
            }
        }
        if let Some(vehicle) = &mut self.vehicle {
            // The pinned Python ret.buttonEvents self-copy clears its existing Cap'n Proto readers.
            // Preserve that source behavior when the vehicle-bus infotainment list is rebuilt.
            buttons.fill((Button::Unknown, false));
            let mut tpms = ret.reborrow().init_tpms();
            let pressure = |p: &mut Parser, s: &str| -> Result<f32, Error> {
                let value = p.signal_lazy("VCSEC_TPMSDisplay", s, now)?;
                f32_value(if value < 255. * 0.025 {
                    (value * 14.5037738 * 10.).round_ties_even() / 10.
                } else {
                    0.
                })
            };
            tpms.set_fl(pressure(vehicle, "VCSEC_TPMSDisplayPressureFL")?);
            tpms.set_fr(pressure(vehicle, "VCSEC_TPMSDisplayPressureFR")?);
            tpms.set_rl(pressure(vehicle, "VCSEC_TPMSDisplayPressureRL")?);
            tpms.set_rr(pressure(vehicle, "VCSEC_TPMSDisplayPressureRR")?);
            let prev = self.extras.infotainment_3_finger_press;
            self.extras.infotainment_3_finger_press = vehicle
                .signal("UI_status2", "UI_activeTouchPoints")?
                .to_i32()
                .ok_or(Error::Numeric)?;
            buttons.extend(changes(
                self.extras.infotainment_3_finger_press,
                prev,
                3,
                Button::Lkas,
            ));
        }
        let mut events = ret
            .reborrow()
            .init_button_events(u32::try_from(buttons.len()).map_err(|_| Error::Numeric)?);
        for (i, (b, p)) in buttons.iter().enumerate() {
            let mut event = events
                .reborrow()
                .get(u32::try_from(i).map_err(|_| Error::Numeric)?);
            event.set_type(*b);
            event.set_pressed(*p);
        }
        let valid = self.party.can_valid()
            && self.autopilot.can_valid()
            && self.vehicle.as_mut().is_none_or(|v| v.can_valid());
        ret.set_can_valid(valid);
        ret.set_can_timeout(
            self.party.bus_timeout()
                || self.autopilot.bus_timeout()
                || self.vehicle.as_ref().is_some_and(|v| v.bus_timeout()),
        );
        let reader = ret.reborrow_as_reader();
        let cluster = reader.get_v_ego_cluster();
        if cluster == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(reader.get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        let current = ret.reborrow_as_reader().get_v_ego_cluster();
        let last = self
            .out
            .get_root_as_reader::<car_state::Reader>()?
            .get_v_ego_cluster();
        ret.set_v_ego_cluster(if current > last {
            current - 0.
        } else if current < last {
            current + 0.
        } else {
            last
        });
        let reader = ret.reborrow_as_reader();
        if reader.get_cruise_state()?.get_speed_cluster() == 0. {
            let speed = reader.get_cruise_state()?.get_speed();
            ret.reborrow().get_cruise_state()?.set_speed_cluster(speed);
        }
        ret.set_button_enable(button_enable(
            self.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_parsers();
        Ok(message)
    }
}
