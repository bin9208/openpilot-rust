use crate::{
    config::{Config, Torque, Tuning},
    inputs::{CarState, Inputs},
    interface::ControlInterface,
    parameters::Parameters,
    torque_neural::Neural,
    Error,
};
use openpilot_control_policy::{
    math::clip,
    pid::{Gain, Pid, Step},
    vehicle::VehicleModel,
};

#[derive(Clone, Copy)]
pub struct Request<'a> {
    pub config: &'a Config,
    pub vm: &'a VehicleModel,
    pub input: &'a Inputs,
    pub active: bool,
    pub curvature: f64,
    pub limited: bool,
    pub curvature_limited: bool,
}

#[derive(Default)]
pub struct Log {
    pub active: bool,
    pub angle: f64,
    pub rate: f64,
    pub desired: f64,
    pub error: f64,
    pub p: f64,
    pub i: f64,
    pub d: f64,
    pub f: f64,
    pub output: f64,
    pub actual_accel: f64,
    pub desired_accel: f64,
    pub saturated: bool,
}
pub struct Lateral {
    pub saturation: f64,
    pub limit: f64,
    pub pid: Option<Pid>,
    pub torque: Option<Torque>,
    pub defaults: Option<Torque>,
    pub custom: i32,
    pub frame: u64,
    pub neural: Option<Neural>,
}
impl Lateral {
    pub fn new(
        config: &Config,
        interface: &ControlInterface,
        params: &mut impl Parameters,
    ) -> Result<Self, Error> {
        let mut out = Self {
            saturation: 0.,
            limit: config.saturation_time,
            pid: None,
            torque: None,
            defaults: None,
            custom: 0,
            frame: 0,
            neural: None,
        };
        if !config.angle {
            match &config.tuning {
                Tuning::Pid(gains) => out.pid = Some(Pid::new(gains.clone(), [-1., 1.])),
                Tuning::Torque(torque) => {
                    out.pid = Some(Pid::new(torque.gains.clone(), [-1., 1.]));
                    out.torque = Some(torque.clone());
                    out.defaults = Some(torque.clone());
                    out.custom = params.integer("LateralTorqueCustom")?;
                    out.neural = Some(Neural::new(config, interface, params)?);
                }
                Tuning::Other => return Err(Error::Contract("unsupported lateral tuning")),
            }
        }
        Ok(out)
    }
    pub fn reset(&mut self) {
        self.saturation = 0.;
        if self.torque.is_none() {
            if let Some(pid) = &mut self.pid {
                pid.reset();
            }
        }
    }
    pub(crate) fn saturation(
        &mut self,
        saturated: bool,
        car: &CarState,
        limited: bool,
        curvature_limited: bool,
        minimum: f64,
    ) -> bool {
        if (saturated || curvature_limited)
            && car.speed > minimum
            && !limited
            && !car.steering_pressed
        {
            self.saturation += 0.01;
        } else {
            self.saturation -= 0.01;
        }
        self.saturation = clip(self.saturation, 0., self.limit);
        self.saturation > self.limit - 1e-3
    }
    pub fn live(&mut self, input: &Inputs) {
        if self.custom <= 0 && input.torque_checks && input.torque.use_params {
            if let Some(torque) = &mut self.torque {
                torque.factor = input.torque.factor;
                torque.offset = input.torque.offset;
                torque.friction = input.torque.friction;
            }
        }
    }
    pub(crate) fn refresh(&mut self, params: &mut impl Parameters) -> Result<(), Error> {
        self.frame += 1;
        if !self.frame.is_multiple_of(10) {
            return Ok(());
        }
        let custom = params.integer("LateralTorqueCustom")?;
        let torque = self
            .torque
            .as_mut()
            .ok_or(Error::Contract("torque tuning"))?;
        let defaults = self
            .defaults
            .as_ref()
            .ok_or(Error::Contract("torque defaults"))?;
        if custom > 0 {
            torque.factor = (params.float("LateralTorqueAccelFactor")? * 0.001) as f32;
            torque.friction = (params.float("LateralTorqueFriction")? * 0.001) as f32;
            let p = params.float("LateralTorqueKpV")? * 0.01;
            let i = params.float("LateralTorqueKiV")? * 0.01;
            let f = params.float("LateralTorqueKf")? * 0.01;
            let d = params.float("LateralTorqueKd")? * 0.01;
            let pid = self.pid.as_mut().ok_or(Error::Contract("torque PID"))?;
            pid.gains.p = Gain::constant(p);
            pid.gains.i = Gain::constant(i);
            pid.gains.f = f;
            pid.gains.d = Gain::constant(d);
            torque.offset = defaults.offset;
        } else if self.custom > 1 {
            torque.factor = defaults.factor;
            torque.friction = defaults.friction;
            torque.offset = defaults.offset;
        }
        self.custom = custom;
        Ok(())
    }
    pub fn update(
        &mut self,
        interface: &ControlInterface,
        params: &mut impl Parameters,
        request: Request<'_>,
    ) -> Result<(f64, f64, Log), Error> {
        let Request {
            config,
            vm,
            input,
            active,
            curvature,
            limited,
            curvature_limited,
        } = request;
        let cs = &input.car;
        let live = &input.live;
        if config.angle {
            let angle = if active {
                vm.steer(-curvature, cs.speed, live.roll)?.to_degrees() + live.offset
            } else {
                cs.steer_angle
            };
            let log = Log {
                active,
                angle: cs.steer_angle,
                desired: if cs.steering_pressed {
                    cs.steer_angle
                } else {
                    angle
                },
                saturated: self.saturation(
                    (angle - cs.steer_angle).abs() > 3.,
                    cs,
                    false,
                    curvature_limited,
                    5.,
                ),
                ..Log::default()
            };
            return Ok((0., angle, log));
        }
        if self.torque.is_some() {
            return self.torque_update(interface, params, request);
        }
        let no_offset = vm.steer(-curvature, cs.speed, live.roll)?.to_degrees();
        let angle = no_offset + live.offset;
        let mut log = Log {
            angle: cs.steer_angle,
            rate: cs.steer_rate,
            desired: angle,
            error: angle - cs.steer_angle,
            ..Log::default()
        };
        let pid = self.pid.as_mut().ok_or(Error::Contract("lateral PID"))?;
        let output = if active {
            let mut step = Step::new(
                log.error,
                cs.speed,
                interface.identity.steer_feedforward(no_offset, cs.speed),
            );
            step.driver_override = cs.steering_pressed;
            let output = pid.update(step)?;
            log.active = true;
            log.p = pid.p;
            log.i = pid.i;
            log.f = pid.f;
            log.output = output;
            log.saturated = self.saturation(
                1. - output.abs() < 1e-3,
                cs,
                limited,
                curvature_limited,
                10.,
            );
            output
        } else {
            pid.reset();
            0.
        };
        Ok((output, angle, log))
    }
}
