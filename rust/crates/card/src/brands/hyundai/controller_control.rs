use super::{
    authority::{AngleAuthority, AuthorityInput},
    controller_settings::Settings,
    flags as f,
    limits::{self, AngleInput},
    state::State,
    Error,
};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_control::{self, h_u_d_control::VisualAlert};

pub struct Command {
    pub torque: f64,
    pub request: bool,
    pub angle: f64,
    pub authority: f64,
    pub accel: f64,
    pub stopping: bool,
    pub speed: f64,
    pub warning: bool,
    pub sys_state: f64,
    pub lane_warning: [f64; 2],
}

#[derive(Default)]
pub struct Steering {
    pub torque: f64,
    pub angle: f64,
    count: i32,
    pub authority: AngleAuthority,
    active_carrot: i16,
    haptic_end: i64,
    pub blink: bool,
}

impl Steering {
    pub fn update(
        &mut self,
        input: (car_control::Reader<'_>, &State, Option<f64>),
        settings: &Settings,
        frame: u32,
    ) -> Result<Command, Error> {
        let (cc, state, y_std) = input;
        let cs = state
            .out
            .get_root_as_reader::<openpilot_cereal::car_capnp::car_state::Reader<'_>>()?;
        let actuators = cc.get_actuators()?;
        let hud = cc.get_hud_control()?;
        let lat = cc.get_lat_active();
        let requested_torque = f64::from(actuators.get_torque()) * settings.limits.max;
        if !requested_torque.is_finite() {
            return Err(Error::Numeric);
        }
        let torque = if lat {
            settings.limits.driver_limit(
                requested_torque.round_ties_even(),
                self.torque,
                f64::from(cs.get_steering_torque()),
            )
        } else {
            0.
        };
        let request = limits::fault_avoidance(
            f64::from(cs.get_steering_angle_deg()).abs() >= 85.,
            lat,
            &mut self.count,
            settings.max_angle_frames,
        );
        let request = if state.config.flags & f::ANGLE_CONTROL != 0 {
            lat
        } else {
            request
        };
        let angle = limits::angle_limit(AngleInput {
            desired: f64::from(actuators.get_steering_angle_deg()),
            previous: self.angle,
            speed: f64::from(cs.get_v_ego_raw()),
            measured: f64::from(cs.get_steering_angle_deg()),
            active: lat,
            wheelbase: state.config.wheelbase,
            ratio: state.config.steer_ratio,
            max_angle: 175.,
            y_std,
        })?;
        let authority = self.authority.update(AuthorityInput {
            active: lat,
            pressed: cs.get_steering_pressed(),
            torque: f64::from(cs.get_steering_torque()),
            threshold: settings.limits.threshold,
            y_std,
        })?;
        self.torque = torque;
        self.angle = angle;
        let warning = matches!(
            hud.get_visual_alert()?,
            VisualAlert::SteerRequired | VisualAlert::Ldw
        );
        let lanes = [hud.get_left_lane_visible(), hud.get_right_lane_visible()];
        let sys_state = if lanes == [true, true] || warning {
            if cc.get_enabled() || warning {
                3.
            } else {
                4.
            }
        } else if lanes[0] {
            5.
        } else if lanes[1] {
            6.
        } else {
            1.
        };
        let depart = if matches!(
            state.config.candidate.as_str(),
            "GENESIS_G90" | "GENESIS_G80"
        ) {
            1.
        } else {
            2.
        };
        let mut lane_warning = [
            if hud.get_left_lane_depart() {
                depart
            } else {
                0.
            },
            if hud.get_right_lane_depart() {
                depart
            } else {
                0.
            },
        ];
        let active = hud.get_active_carrot() == 3 && self.active_carrot != 3;
        self.active_carrot = hud.get_active_carrot();
        let frame64 = i64::from(frame);
        if active && self.haptic_end < 0 {
            self.haptic_end = frame64 + 800;
        } else if !active {
            self.haptic_end = -1;
        }
        if (0..800).contains(&(self.haptic_end - frame64)) && settings.haptic > 0 {
            let time = (frame64 - (self.haptic_end - 800))
                .to_f64()
                .ok_or(Error::Numeric)?
                * 0.01;
            if [(0., 0.5), (1., 1.5), (5., 5.5), (6., 6.5), (7.5, 8.)]
                .iter()
                .any(|(start, end)| *start <= time && time < *end)
            {
                lane_warning = [f64::from(settings.haptic); 2];
            }
        }
        if frame64 >= self.haptic_end {
            self.haptic_end = -1;
        }
        if frame.is_multiple_of(100) {
            self.blink = true;
        } else if frame % 100 == 50 {
            self.blink = false;
        }
        Ok(Command {
            torque,
            request,
            angle,
            authority,
            accel: f64::from(actuators.get_accel()).clamp(-4., 2.5),
            stopping: actuators.get_long_control_state()?
                == car_control::actuators::LongControlState::Stopping,
            speed: f64::from(hud.get_set_speed()) * if state.metric { 3.6 } else { 1. / 0.44704 },
            warning,
            sys_state,
            lane_warning,
        })
    }
}
