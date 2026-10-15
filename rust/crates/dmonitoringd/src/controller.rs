use crate::{wire, Error};
use openpilot_cereal::{
    car_capnp::{car_params, car_state::GearShifter},
    log_capnp::event,
};
use openpilot_desire::command::CommandReader;
use openpilot_messaging::state::State;
use openpilot_monitoring::{context::*, DriverMonitoring2};
use openpilot_params::Params;

pub const TOPICS: &[&str] = &[
    "driverStateV2",
    "liveCalibration",
    "carState",
    "selfdriveState",
    "modelV2",
    "radarState",
    "carParams",
];
pub struct Controller {
    pub monitoring: DriverMonitoring2,
    demo: bool,
    rhd_saved: bool,
    enabled: bool,
    frame: u32,
    traffic: TrafficContext,
    inputs: InteractionEdges,
    cancel: CancelPressSequence,
    automatic: AutomaticCancelFilter,
    camera: CameraAvailability,
    touch: SteeringTouchEvidence,
    covered: bool,
    allow_speed_buttons: bool,
    filter_automatic: bool,
    last_fresh: f64,
    bluetooth: CommandReader,
    next_mode_check: f64,
    last_camera_ok: bool,
    response_pending: bool,
    strict: bool,
    clear: bool,
}
pub struct Publication {
    pub bytes: Vec<u8>,
    pub frame_id: u32,
}
pub fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    now.tv_sec as f64 + now.tv_nsec as f64 / 1e9
}
fn setting(params: &Params, key: &str) -> Result<i64, Error> {
    Ok(params
        .get(key)?
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}
fn fresh(now: f64, time: f64) -> bool {
    time.is_finite() && (0. ..0.25).contains(&(now - time))
}
macro_rules! read {
    ($state:expr,$name:literal,$kind:ident) => {{
        let event::$kind(value) = $state.topic($name)?.event()?.which()? else {
            return Err(Error::Contract(concat!("expected ", $name)));
        };
        value?
    }};
}
impl Controller {
    pub fn new(params: &Params) -> Result<Self, Error> {
        let now = monotonic();
        let mut monitoring = DriverMonitoring2::new(
            params.get_bool("IsRhdDetected")?,
            params.get_bool("AlwaysOnDM")?,
            setting(params, "DriverMonitoringMode")? == 1,
        );
        monitoring.monitor.too_distracted = params.get_bool("DriverTooDistracted")?;
        Ok(Self {
            monitoring,
            demo: params.get_bool("IsDriverViewEnabled")?,
            rhd_saved: params.get_bool("IsRhdDetected")?,
            enabled: params.get_bool("DriverMonitoringEnabled")?
                && !params.get_bool("DriverMonitoringSessionDisabled")?,
            frame: 0,
            traffic: TrafficContext::default(),
            inputs: InteractionEdges::default(),
            cancel: CancelPressSequence::default(),
            automatic: AutomaticCancelFilter::default(),
            camera: CameraAvailability::default(),
            touch: SteeringTouchEvidence::default(),
            covered: false,
            allow_speed_buttons: false,
            filter_automatic: true,
            last_fresh: f64::NEG_INFINITY,
            next_mode_check: now + 0.5,
            last_camera_ok: false,
            response_pending: false,
            strict: true,
            clear: false,
            bluetooth: CommandReader::new(
                std::path::Path::new("/dev/shm/carrot-bluetooth"),
                "attention",
                now,
            ),
        })
    }
    pub fn configure_car_params(
        &mut self,
        cp: car_params::Reader<'_>,
        params: &Params,
    ) -> Result<(), Error> {
        let hyundai = cp.get_brand()?.to_str().ok() == Some("hyundai");
        self.covered = hyundai
            && setting(params, "EnableCornerRadar")? > 0
            && cp.get_ext_flags() & ((1 << 12) | (1 << 13) | (1 << 14)) != 0;
        self.allow_speed_buttons =
            cp.get_openpilot_longitudinal_control() || setting(params, "SpeedFromPCM")? == 0;
        let filter = !hyundai || !cp.get_openpilot_longitudinal_control();
        if filter != self.filter_automatic {
            self.automatic = AutomaticCancelFilter::default();
        }
        self.filter_automatic = filter;
        Ok(())
    }
    pub fn refresh(&mut self, params: &Params) -> Result<(), Error> {
        let now = monotonic();
        if now >= self.next_mode_check {
            self.monitoring
                .set_experimental(setting(params, "DriverMonitoringMode")? == 1);
            self.next_mode_check = now + 0.5;
        }
        if self.frame.is_multiple_of(40) {
            let enabled = params.get_bool("DriverMonitoringEnabled")?
                && !params.get_bool("DriverMonitoringSessionDisabled")?;
            if enabled && !self.enabled {
                self.monitoring = DriverMonitoring2::new(
                    params.get_bool("IsRhdDetected")?,
                    params.get_bool("AlwaysOnDM")?,
                    setting(params, "DriverMonitoringMode")? == 1,
                );
                self.monitoring.monitor.too_distracted = params.get_bool("DriverTooDistracted")?;
                self.strict = true;
                self.clear = false;
                self.response_pending = false;
                self.traffic = TrafficContext::default();
                self.inputs = InteractionEdges::default();
                self.cancel = CancelPressSequence::default();
                self.automatic = AutomaticCancelFilter::default();
                self.camera = CameraAvailability::default();
                self.touch = SteeringTouchEvidence::default();
            }
            self.enabled = enabled;
            self.monitoring.monitor.always_on = params.get_bool("AlwaysOnDM")?;
            self.demo = params.get_bool("IsDriverViewEnabled")?;
            self.rhd_saved = params.get_bool("IsRhdDetected")?;
        }
        Ok(())
    }
    pub fn control_packet(&mut self, bytes: &[u8], now: f64) -> Result<(), Error> {
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let root = message.get_root::<event::Reader>()?;
        if self.filter_automatic && fresh(now, root.get_log_mono_time() as f64 / 1e9) {
            if let event::CarControl(cc) = root.which()? {
                self.automatic.record(
                    root.get_log_mono_time() as f64 / 1e9,
                    cc?.get_cruise_control()?.get_cancel(),
                );
            }
        }
        Ok(())
    }
    pub fn car_packet(&mut self, bytes: &[u8], now: f64, params: &Params) -> Result<(), Error> {
        let message = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let root = message.get_root::<event::Reader>()?;
        let event::CarState(cs) = root.which()? else {
            return Err(Error::Contract("expected carState input queue"));
        };
        let cs = cs?;
        let time = root.get_log_mono_time() as f64 / 1e9;
        if !(root.get_valid() && cs.get_can_valid() && fresh(now, time)) {
            self.cancel.reset_input_stream();
            self.automatic.reset_input_stream();
            self.last_fresh = f64::NEG_INFINITY;
            return Ok(());
        }
        let gap = time - self.last_fresh;
        if self.last_fresh.is_finite() && !(0. ..0.25).contains(&gap) {
            self.cancel.reset_input_stream();
            self.automatic.reset_input_stream();
        }
        self.last_fresh = time;
        let raw: Vec<_> = cs
            .get_button_events()?
            .iter()
            .map(|b| Ok((button_name(b.get_type()?).to_owned(), b.get_pressed())))
            .collect::<Result<_, Error>>()?;
        let physical = if self.filter_automatic {
            self.automatic.filter_buttons(time, raw)
        } else {
            raw
        };
        if self.enabled && self.cancel.update(time, &physical) {
            self.enabled = false;
            params.put_bool("DriverMonitoringSessionDisabled", true)?;
        }
        let buttons: Vec<_> = physical
            .into_iter()
            .filter(|(kind, _)| self.allow_speed_buttons || !InteractionEdges::speed_button(kind))
            .collect();
        self.response_pending |= self.inputs.update(
            time,
            cs.get_gas_pressed(),
            cs.get_brake_pressed(),
            cs.get_steering_pressed(),
            &buttons,
        );
        Ok(())
    }
    pub fn prepare(&mut self, state: &State) -> Result<Option<Publication>, Error> {
        self.prepare_at(state, runtime_clock(state))
    }
    pub fn prepare_at(&mut self, state: &State, now: f64) -> Result<Option<Publication>, Error> {
        // Dispatcher always publishes, including camera absence and disabled/paused periods.
        let valid = state.all_checks(&["carState", "selfdriveState"])?
            && replay_fresh(state, &["carState", "selfdriveState"], now)
            && state.topic("carState")?.seen
            && read!(state, "carState", CarState).get_can_valid();
        if now - self.last_fresh >= 0.25 {
            self.cancel.reset_input_stream();
            self.automatic.reset_input_stream();
        }
        if self.bluetooth.read(valid && self.enabled, now).is_some() {
            self.inputs.last_response = Some(now);
            self.response_pending = true;
        }
        let response = std::mem::take(&mut self.response_pending);
        let mut paused = false;
        let mut parked = false;
        if state.topic("carState")?.seen && state.topic("selfdriveState")?.seen {
            let cs = read!(state, "carState", CarState);
            let sd = read!(state, "selfdriveState", SelfdriveState);
            let current = valid
                && ["carState", "selfdriveState"].iter().all(|name| {
                    state
                        .topic(name)
                        .is_ok_and(|t| fresh(now, t.log_mono_time as f64 / 1e9))
                });
            paused = !self.demo
                && current
                && cs.get_soft_hold_active() > 0
                && cs.get_standstill()
                && !cs.get_gas_pressed()
                && matches!(
                    cs.get_gear_shifter(),
                    Ok(GearShifter::Drive | GearShifter::Low)
                )
                && cs.get_v_ego().is_finite()
                && cs.get_v_ego_raw().is_finite()
                && cs.get_v_ego().abs() < 0.1
                && cs.get_v_ego_raw().abs() < 0.1;
            parked = !self.demo
                && current
                && cs.get_gear_shifter() == Ok(GearShifter::Park)
                && cs.get_standstill()
                && cs.get_v_ego_raw() == 0.
                && cs.get_v_ego().is_finite()
                && cs.get_v_ego().abs() < 0.01
                && !sd.get_enabled()
                && !sd.get_active();
        }
        let camera_ok = if paused || (!self.enabled && !self.demo) {
            false
        } else {
            self.camera.update(
                now,
                wire::camera_usable(
                    state,
                    self.monitoring.monitor.wheel_on_right,
                    self.demo,
                    now,
                )?,
            )
        };
        self.last_camera_ok = camera_ok;
        if self.enabled && !paused {
            let (strict, clear) = self.road_context(state, now)?;
            self.monitoring
                .configure_context(now, camera_ok, strict, clear);
            if valid {
                let cs = read!(state, "carState", CarState);
                let touch = cs.get_steering_touch()?;
                let (held, edge) = self.touch.update(
                    now,
                    touch.get_available(),
                    touch.get_valid(),
                    touch.get_sample_mono_time(),
                    touch.get_touched(),
                    valid,
                );
                if edge {
                    self.inputs.last_response = Some(
                        self.inputs
                            .last_response
                            .unwrap_or(f64::NEG_INFINITY)
                            .max(touch.get_sample_mono_time() as f64 / 1e9),
                    );
                }
                if let Some(time) = self.inputs.last_response {
                    self.monitoring.record_interaction(time);
                }
                if !camera_ok {
                    let sd = read!(state, "selfdriveState", SelfdriveState);
                    let response = held || response || edge;
                    self.monitoring.run_without_camera(
                        response,
                        sd.get_enabled(),
                        cs.get_v_ego() < 2.8,
                        !matches!(
                            cs.get_gear_shifter(),
                            Ok(GearShifter::Drive | GearShifter::Low)
                        ),
                    );
                }
            }
            if camera_ok && state.topic("driverStateV2")?.updated && (valid || self.demo) {
                self.monitoring
                    .run_step(&wire::input(state, wire::driver(state)?, self.demo)?)?;
            }
            self.monitoring.update_parked_reset(now, parked);
        } else if paused {
            self.monitoring.update_parked_reset(now, false);
        } else if self.demo && camera_ok && state.topic("driverStateV2")?.updated {
            self.monitoring
                .run_step(&wire::input(state, wire::driver(state)?, true)?)?;
        }
        let disabled = !self.enabled || paused;
        let mut blank = DriverMonitoring2::new(self.rhd_saved, false, self.monitoring.experimental);
        blank.monitor.active_policy = openpilot_monitoring::Policy::Wheeltouch;
        blank.monitor.wheel_on_right = blank.monitor.wheel_on_right_default;
        let packet_monitor = if !(self.enabled || self.demo && camera_ok) {
            &blank
        } else {
            &self.monitoring
        };
        let bytes = packet_monitor.state_packet(
            if !self.enabled {
                true
            } else {
                valid || (self.demo && camera_ok)
            },
            (now * 1e9) as u64,
            disabled,
            if paused {
                false
            } else if !self.enabled {
                self.demo && !camera_ok
            } else {
                !camera_ok
            },
            (self.traffic.strict_until - now).max(0.),
        )?;
        let frame_id = self.frame;
        self.frame = self.frame.wrapping_add(1);
        Ok(Some(Publication { bytes, frame_id }))
    }
    fn road_context(&mut self, state: &State, now: f64) -> Result<(bool, bool), Error> {
        if !state.all_checks(&["modelV2", "radarState"])?
            || !replay_fresh(state, &["modelV2", "radarState"], now)
            || !state.topic("radarState")?.seen
            || !state.topic("modelV2")?.seen
            || !state.topic("carState")?.seen
        {
            self.strict = true;
            self.clear = false;
            self.traffic.update(now, &[], false, false, self.covered);
            return Ok((true, false));
        }
        let radar = read!(state, "radarState", RadarState);
        let errors = radar.get_radar_errors()?;
        let healthy = !(errors.get_can_error()
            || errors.get_radar_fault()
            || errors.get_wrong_config()
            || errors.get_radar_unavailable_temporary());
        if healthy && !state.topic("radarState")?.updated {
            return Ok((self.strict, self.clear));
        }
        let mut objects = Vec::new();
        let mut add = |p: openpilot_cereal::log_capnp::radar_state::lead_data::Reader<'_>| {
            if p.get_status() {
                objects.push(ObjectObservation {
                    x: p.get_d_rel() as f64,
                    y: p.get_d_path() as f64,
                    speed: p.get_v_lead() as f64,
                    relative_speed: p.get_v_rel() as f64,
                });
            }
        };
        add(radar.get_lead_one()?);
        add(radar.get_lead_two()?);
        add(radar.get_lead_left()?);
        add(radar.get_lead_right()?);
        for list in [
            radar.get_leads_center()?,
            radar.get_leads_left()?,
            radar.get_leads_right()?,
            radar.get_leads_left2()?,
            radar.get_leads_right2()?,
            radar.get_leads_cut_in()?,
        ] {
            for lead in list.iter() {
                add(lead);
            }
        }
        let cs = read!(state, "carState", CarState);
        let model = read!(state, "modelV2", ModelV2);
        let z = model.get_orientation_rate()?.get_z()?;
        let probs = model.get_lane_line_probs()?;
        let straight = cs.get_steering_angle_deg().abs() < 5.
            && cs.get_a_ego().abs() < 0.5
            && z.len() >= 10
            && z.iter().take(10).all(|v| v.abs() < 0.01)
            && probs.len() == 4
            && probs.get(1).min(probs.get(2)) > 0.8
            && !cs.get_left_blinker()
            && !cs.get_right_blinker();
        let result = self
            .traffic
            .update(now, &objects, healthy, straight, self.covered);
        self.strict = result.0;
        self.clear = result.1;
        Ok(result)
    }
    pub fn after_publish(&mut self, frame_id: u32, params: &Params) -> Result<(), Error> {
        if frame_id.is_multiple_of(6000)
            && !self.demo
            && self.enabled
            && self.last_camera_ok
            && self.monitoring.monitor.wheelpos_offsetter.filtered_stat.n > 300
        {
            params.put_bool("IsRhdDetected", self.monitoring.monitor.wheel_on_right)?;
        }
        Ok(())
    }
}
fn button_name(kind: openpilot_cereal::car_capnp::car_state::button_event::Type) -> &'static str {
    use openpilot_cereal::car_capnp::car_state::button_event::Type::*;
    match kind {
        AccelCruise => "accelCruise",
        DecelCruise => "decelCruise",
        ResumeCruise => "resumeCruise",
        SetCruise => "setCruise",
        GapAdjustCruise => "gapAdjustCruise",
        Cancel => "cancel",
        MainCruise => "mainCruise",
        Lkas => "lkas",
        LfaButton => "lfaButton",
        _ => "other",
    }
}

pub fn runtime_clock(state: &State) -> f64 {
    if std::env::var_os("REPLAY").is_some() {
        let route = ["driverStateV2", "modelV2", "carState", "selfdriveState"]
            .iter()
            .filter_map(|name| state.topic(name).ok().map(|t| t.log_mono_time))
            .max()
            .unwrap_or(0) as f64
            / 1e9;
        if route.is_finite() && route > 0. {
            return route;
        }
    }
    monotonic()
}

fn replay_fresh(state: &State, names: &[&str], now: f64) -> bool {
    std::env::var_os("REPLAY").is_none()
        || names.iter().all(|name| {
            state.topic(name).is_ok_and(|t| {
                let age = now - t.log_mono_time as f64 / 1e9;
                age >= 0. && age < 10. / t.service.frequency
            })
        })
}
