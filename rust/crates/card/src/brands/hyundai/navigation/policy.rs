//! Final stock-navigation input consumption and section-average policy.
use super::{
    section_average::SectionAverage, speedcam::SpeedcamPolicy, EventType, Navigation, Output,
};
use crate::brands::hyundai::{parameters::setting_int, Error};
use openpilot_cereal::car_capnp::car_state::button_event::Type;
use openpilot_params::Params;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub mobile_decel: bool,
    pub skip_box: bool,
    pub skip_mobile: bool,
    pub skip_unknown: bool,
    pub cancel_bump: bool,
    pub bump_decel: bool,
    pub bump_speed: f64,
    pub bump_time: f64,
    pub bump_end: f64,
    pub decel_rate: f64,
    pub section_control: bool,
    pub safety_factor: f64,
    pub long_press: i64,
}
#[derive(Default, serde::Deserialize, serde::Serialize)]
pub struct Policy {
    pub section: SectionAverage,
    pub accel_swallow: bool,
    pub gas_count: u32,
    pub gas_consumed: bool,
    pub input_consumed: bool,
    pub decel_count: u32,
    pub decel_hold: bool,
    pub decel_long_fired: bool,
    pub time: f64,
}
#[derive(Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Context {
    pub accel_rising: bool,
    pub gas_tap: bool,
    pub engaged: bool,
    pub cruise_kph: f64,
    pub v_ego: f64,
    pub cluster_ratio: f64,
}
impl Config {
    pub fn read(params: &Params, cameras: &mut SpeedcamPolicy) -> Result<Self, Error> {
        let mobile_decel = setting_int(params, "AutoNaviSpeedCtrlMode")? >= 3;
        let cancel = params.get_bool("VehicleNaviDecelCancel")?;
        let skip_box = cancel && params.get_bool("VehicleNaviDecelCancelBox")?;
        let skip_mobile = cancel && params.get_bool("VehicleNaviDecelCancelMobileZone")?;
        let cancel_bump = cancel && params.get_bool("VehicleNaviDecelCancelBump")?;
        let skip_unknown = cancel && params.get_bool("VehicleNaviDecelCancelUnknown")?;
        cameras.early_lead = cancel && params.get_bool("VehicleNaviDecelCancelEarlyWarning")?;
        Ok(Self {
            mobile_decel,
            skip_box,
            skip_mobile,
            skip_unknown,
            cancel_bump,
            bump_decel: setting_int(params, "AutoNaviSpeedCtrlMode")? >= 2,
            bump_speed: f64::from(setting_int(params, "AutoNaviSpeedBumpSpeed")?),
            bump_time: f64::from(setting_int(params, "AutoNaviSpeedBumpTime")?),
            bump_end: f64::from(
                setting_int(params, "AutoNaviSpeedBumpEndDistance")?.clamp(0, 5000),
            ) * 0.01,
            decel_rate: f64::from(setting_int(params, "AutoNaviSpeedDecelRate")?) * 0.01,
            section_control: params.get_bool("VehicleNaviSectionAvgControl")?,
            safety_factor: f64::from(setting_int(params, "AutoNaviSpeedSafetyFactor")?) * 0.01,
            long_press: i64::from(setting_int(params, "CruiseButtonLongDelay")?),
        })
    }
}
impl Policy {
    pub fn gas_tap(&mut self, pressed: bool) -> bool {
        if pressed {
            self.gas_count = self.gas_count.saturating_add(1);
            return false;
        }
        let tap = 0 < self.gas_count && self.gas_count < 40;
        self.gas_count = 0;
        tap
    }
    pub fn decel_long_press(&mut self, button: u8, config: &Config) -> bool {
        if button != 2 {
            self.decel_count = 0;
            return false;
        }
        self.decel_count = self.decel_count.saturating_add(1);
        i64::from(self.decel_count) == config.long_press + 1
    }
    pub fn consume(&mut self, c: Context) {
        self.input_consumed = true;
        self.accel_swallow = c.accel_rising;
        self.gas_consumed = c.gas_tap && !c.accel_rising;
    }
    pub fn camera(
        &mut self,
        nav: &mut Navigation,
        output: &Output,
        camera: bool,
        warning: (bool, f64),
        c: Context,
        config: &Config,
    ) -> Result<bool, Error> {
        self.gas_consumed = false;
        self.input_consumed = false;
        if nav.wrapped || nav.mode == 0 {
            nav.camera_policy.reset_warning();
            return Ok(camera);
        }
        if !warning.1.is_finite() {
            return Err(Error::Numeric);
        }
        let (suppress, consume) = nav.camera_policy.update(
            warning.0,
            warning.1.round_ties_even(),
            nav.total_distance,
            c.engaged && c.v_ego > 5. && (c.accel_rising || c.gas_tap),
            config.mobile_decel,
            config.skip_box,
            config.skip_mobile,
            config.skip_unknown && !output.section_active,
        );
        if consume {
            self.consume(c);
        }
        Ok(!suppress && camera)
    }
    pub fn bump_target(&self, distance: f64, config: &Config) -> f64 {
        let safe = config.bump_speed / 3.6;
        let decel_dist = distance - safe * config.bump_time;
        if decel_dist <= 0. {
            return config.bump_speed;
        }
        config
            .bump_speed
            .max(250_f64.min((safe.powf(2.) + 2. * config.decel_rate * decel_dist).sqrt() * 3.6))
    }
    pub fn bump(&mut self, nav: &mut Navigation, output: &mut Output, c: Context, config: &Config) {
        if nav.wrapped || nav.mode == 0 {
            return;
        }
        let live: Vec<_> = nav
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.kind == EventType::Bump && e.target > nav.total_distance && !e.cancelled
            })
            .map(|(i, _)| i)
            .collect();
        output.bump_distance = live
            .first()
            .map_or(0., |i| nav.events[*i].target - nav.total_distance);
        if live.is_empty()
            || !config.cancel_bump
            || !config.bump_decel
            || self.input_consumed
            || !(c.accel_rising || c.gas_tap)
            || !c.engaged
            || c.v_ego <= 5.
        {
            return;
        }
        let distance = output.bump_distance;
        if distance <= config.bump_end || self.bump_target(distance, config) >= c.cruise_kph {
            return;
        }
        nav.events[live[0]].cancelled = true;
        output.bump_distance = live
            .get(1)
            .map_or(0., |i| nav.events[*i].target - nav.total_distance);
        self.consume(c);
    }
    pub fn section(
        &mut self,
        nav: &Navigation,
        output: &mut Output,
        c: Context,
        long_press: bool,
        config: &Config,
    ) {
        self.time += 0.01;
        let active = !nav.wrapped
            && config.section_control
            && nav.mode != 0
            && output.section_active
            && output.speed > 0.;
        let speed = if active {
            output.speed.round_ties_even()
        } else {
            0.
        };
        self.section
            .update(active, speed, self.time, nav.total_distance);
        if !active {
            return;
        }
        if !self.section.unlocked
            && !self.input_consumed
            && (c.accel_rising || c.gas_tap)
            && c.engaged
            && c.v_ego > 5.
            && c.cruise_kph > speed * config.safety_factor
        {
            self.section.unlock();
            self.consume(c);
        } else if self.section.unlocked && long_press {
            self.section.lock();
        }
        if !self.section.unlocked {
            return;
        }
        let ratio = if c.cluster_ratio > 0.5 {
            c.cluster_ratio
        } else {
            1.
        };
        let speed_allowed = self.section.allowance_kph(self.time, nav.total_distance)
            / super::section_average::DISTANCE_SCALE
            / ratio
            / config.safety_factor.max(0.5);
        if speed_allowed > speed {
            output.speed = speed_allowed;
        }
        output.speed_limit_distance = nav
            .events
            .iter()
            .filter(|e| {
                e.kind == EventType::Camera && e.speed == speed && e.target > nav.total_distance
            })
            .map(|e| e.target - nav.total_distance)
            .reduce(f64::min)
            .unwrap_or(0.);
    }
    pub fn filter_buttons(
        &mut self,
        events: Vec<(bool, Type)>,
        long_press: bool,
    ) -> Vec<(bool, Type)> {
        let mut kept = Vec::new();
        if long_press && self.decel_hold {
            self.decel_long_fired = true;
        }
        for (pressed, kind) in events {
            if self.accel_swallow && kind == Type::AccelCruise {
                if !pressed {
                    self.accel_swallow = false;
                }
                continue;
            }
            if kind != Type::DecelCruise {
                kept.push((pressed, kind));
            } else if pressed {
                if self.section.unlocked {
                    self.decel_hold = true;
                    self.decel_long_fired = false;
                } else {
                    kept.push((pressed, kind));
                }
            } else if self.decel_hold {
                if !self.decel_long_fired {
                    kept.extend([(true, Type::DecelCruise), (pressed, kind)]);
                }
                self.decel_hold = false;
            } else {
                kept.push((pressed, kind));
            }
        }
        kept
    }
}
