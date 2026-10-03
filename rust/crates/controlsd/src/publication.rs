use crate::{
    controller::{Command, Controls},
    inputs::Inputs,
    lateral::Log,
    longitudinal::State,
    parameters::Parameters,
    Error,
};
use capnp::{message::Builder, serialize};
use openpilot_cereal::{
    car_capnp::car_control::actuators::LongControlState,
    log_capnp::{controls_state, event},
};
use openpilot_control_policy::math::minimum;

pub fn state(state: State) -> LongControlState {
    match state {
        State::Off => LongControlState::Off,
        State::Pid => LongControlState::Pid,
        State::Stopping => LongControlState::Stopping,
        State::Starting => LongControlState::Starting,
    }
}
pub fn publish(
    controls: &mut Controls,
    input: &Inputs,
    command: &Command,
    params: &mut impl Parameters,
    mut clock: impl FnMut() -> u64,
) -> Result<[Vec<u8>; 2], Error> {
    let mut car = Builder::new_default();
    {
        let mut event = car.init_root::<event::Builder<'_>>();
        event.set_valid(input.car.can_valid);

        let mut cc = event.init_car_control();
        cc.set_enabled(command.enabled);
        cc.set_lat_active(command.lateral);
        cc.set_long_active(command.longitudinal);
        cc.set_left_blinker(command.left);
        cc.set_right_blinker(command.right);
        {
            let mut actuators = cc.reborrow().init_actuators();
            actuators.set_long_control_state(state(controls.longitudinal.state));
            actuators.set_accel(command.accel);
            actuators.set_a_target(command.target);
            actuators.set_jerk(command.jerk);
            actuators.set_curvature(command.curvature);
            actuators.set_torque(command.torque);
            actuators.set_steering_angle_deg(command.angle);
        }
        cc.set_current_curvature(controls.curvature as f32);
        if let Some(pose) = &controls.pose {
            let mut orientation = cc.reborrow().init_orientation_n_e_d(3);
            for (i, value) in pose.orientation.iter().enumerate() {
                orientation.set(i as u32, *value as f32);
            }
            let mut angular = cc.reborrow().init_angular_velocity(3);
            for (i, value) in pose.angular.iter().enumerate() {
                angular.set(i as u32, *value as f32);
            }
        }
        let desired = if input.carrot_fresh {
            minimum(input.car.cluster, input.carrot.desired)
        } else {
            input.car.cluster
        };
        let mut speed = desired * (1. / 3.6);
        {
            let mut cruise = cc.reborrow().init_cruise_control();
            cruise.set_override(
                command.enabled && !command.longitudinal && controls.config.openpilot_long,
            );
            cruise.set_cancel(
                input.car.cruise_enabled && (!command.enabled || !controls.config.pcm_cruise),
            );
            if let Some(last) = input.longitudinal.speeds.last() {
                cruise.set_resume(command.enabled && input.car.cruise_standstill && *last > 0.1);
                speed = last
                    / if input.car.cluster_ratio > 0.5 {
                        input.car.cluster_ratio
                    } else {
                        1.
                    };
            }
        }
        crate::hud::write(
            controls,
            input,
            command,
            params,
            cc.init_hud_control(),
            desired,
            speed,
        )?;
    }
    controls.feedback(input, command);
    let mut message = Builder::new_default();
    {
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_valid(input.car.can_valid);
        event.set_log_mono_time(clock());
        let mut cs = event.init_controls_state();
        cs.set_curvature(controls.curvature as f32);
        cs.set_longitudinal_plan_mono_time(input.long_time);
        cs.set_lateral_plan_mono_time(input.model_time);
        cs.set_desired_curvature(controls.desired as f32);
        cs.set_long_control_state(state(controls.longitudinal.state));
        cs.set_up_accel_cmd(controls.longitudinal.pid.p as f32);
        cs.set_ui_accel_cmd(controls.longitudinal.pid.i as f32);
        cs.set_uf_accel_cmd(controls.longitudinal.pid.f as f32);
        cs.set_force_decel(
            params.integer("DisableDM")? == 0
                && (input.distracted || input.selfdrive.soft_disabling),
        );
        cs.set_active_lane_line(controls.lane_lines);
        lateral(controls, &command.log, cs.init_lateral_control_state());
    }
    car.get_root::<event::Builder<'_>>()?
        .set_log_mono_time(clock());
    Ok([
        serialize::write_message_to_words(&message),
        serialize::write_message_to_words(&car),
    ])
}
fn lateral(
    controls: &Controls,
    log: &Log,
    builder: controls_state::lateral_control_state::Builder<'_>,
) {
    if controls.config.angle {
        let mut out = builder.init_angle_state();
        out.set_active(log.active);
        out.set_steering_angle_deg(log.angle as f32);
        out.set_steering_angle_desired_deg(log.desired as f32);
        out.set_saturated(log.saturated);
    } else if controls.lateral.torque.is_some() {
        let mut out = builder.init_torque_state();
        out.set_active(log.active);
        out.set_error(log.error as f32);
        out.set_p(log.p as f32);
        out.set_i(log.i as f32);
        out.set_d(log.d as f32);
        out.set_f(log.f as f32);
        out.set_output(log.output as f32);
        out.set_actual_lateral_accel(log.actual_accel as f32);
        out.set_desired_lateral_accel(log.desired_accel as f32);
        out.set_saturated(log.saturated);
    } else {
        let mut out = builder.init_pid_state();
        out.set_active(log.active);
        out.set_steering_angle_deg(log.angle as f32);
        out.set_steering_rate_deg(log.rate as f32);
        out.set_steering_angle_desired_deg(log.desired as f32);
        out.set_angle_error(log.error as f32);
        out.set_p(log.p as f32);
        out.set_i(log.i as f32);
        out.set_f(log.f as f32);
        out.set_output(log.output as f32);
        out.set_saturated(log.saturated);
    }
}
