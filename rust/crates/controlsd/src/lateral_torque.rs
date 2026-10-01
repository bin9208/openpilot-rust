use crate::{
    interface::ControlInterface,
    lateral::{Lateral, Log, Request},
    parameters::Parameters,
    torque_neural::Acceleration,
    Error,
};
use openpilot_control_policy::{math::interp, pid::Step};
impl Lateral {
    pub(crate) fn torque_update(
        &mut self,
        interface: &ControlInterface,
        params: &mut impl Parameters,
        request: Request<'_>,
    ) -> Result<(f64, f64, Log), Error> {
        let Request {
            vm,
            input,
            active,
            curvature,
            limited,
            curvature_limited,
            ..
        } = request;
        self.refresh(params)?;
        let cs = &input.car;
        let live = &input.live;
        if !active {
            return Ok((-0., cs.steer_angle, Log::default()));
        }
        let angle = vm.steer(-curvature, cs.speed, live.roll)?.to_degrees() + live.offset;
        let torque = self
            .torque
            .as_ref()
            .ok_or(Error::Contract("torque tuning"))?;
        let actual_vm = -vm.curvature(
            (cs.steer_angle - live.offset).to_radians(),
            cs.speed,
            live.roll,
        )?;
        let mut actual_jerk = 0.;
        let (actual, deadzone) = if torque.steering_angle {
            if interface.use_nnff || interface.use_nnff_lite {
                actual_jerk =
                    -vm.curvature(cs.steer_rate.to_radians(), cs.speed, 0.)? * cs.speed.powi(2);
            }
            (
                actual_vm,
                vm.curvature(torque.deadzone.to_radians(), cs.speed, 0.)?
                    .abs(),
            )
        } else {
            (interp(cs.speed, &[2., 5.], &[actual_vm, 0.])?, 0.)
        };
        let desired_accel = curvature * cs.speed.powi(2);
        let actual_accel = actual * cs.speed.powi(2);
        let low = interp(cs.speed, &[0., 10., 20., 30.], &[15., 13., 10., 5.])?.powi(2);
        let acceleration = Acceleration {
            desired: desired_accel,
            actual: actual_accel,
            deadzone: deadzone * cs.speed.powi(2),
            actual_jerk,
            setpoint: desired_accel + low * curvature,
            measurement: actual_accel + low * actual,
        };
        let [error, feedforward] = self
            .neural
            .as_mut()
            .ok_or(Error::Contract("torque response"))?
            .response(interface, torque, input, acceleration)?;
        let mut step = Step::new(error, cs.speed, feedforward);
        step.freeze = limited || cs.steering_pressed || cs.speed < 5.;
        let pid = self.pid.as_mut().ok_or(Error::Contract("torque PID"))?;
        let output = pid.update(step)?;
        let mut log = Log {
            active: true,
            error,
            p: pid.p,
            i: pid.i,
            d: pid.d,
            f: pid.f,
            output: -output,
            actual_accel,
            desired_accel,
            ..Log::default()
        };
        log.saturated = self.saturation(
            1. - output.abs() < 1e-3,
            cs,
            limited,
            curvature_limited,
            10.,
        );
        Ok((-output, angle, log))
    }
}
