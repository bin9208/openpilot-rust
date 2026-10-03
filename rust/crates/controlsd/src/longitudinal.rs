use crate::{config::Config, inputs::Inputs, parameters::Parameters, Error};
use openpilot_control_policy::{
    math::{clip, minimum},
    pid::{Gains, Pid, Step},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    Pid,
    Stopping,
    Starting,
}
pub struct Longitudinal {
    pub state: State,
    pub pid: Pid,
    pub last: f64,
    pub correction: f64,
    pub stopping_accel: f64,
    pub reads: u8,
}
impl Longitudinal {
    pub fn new(config: &Config, params: &mut impl Parameters) -> Result<Self, Error> {
        let mut out = Self {
            state: State::Off,
            pid: Pid::new(config.long_gains.clone(), [-1e308, 1e308]),
            last: 0.,
            correction: 0.,
            stopping_accel: 0.,
            reads: 0,
        };
        out.refresh_stop(params)?;
        if config.brand == "hyundai" {
            out.pid.gains = Gains::constants(1., 0., 1.);
        }
        Ok(out)
    }
    fn refresh_stop(&mut self, params: &mut impl Parameters) -> Result<(), Error> {
        let value = params.float("StoppingAccel")?;
        self.stopping_accel =
            clip(if value.is_finite() { value } else { -50. }, -100., -50.) * 0.01;
        Ok(())
    }
    pub fn reset(&mut self) {
        self.pid.reset();
        self.correction = 0.;
    }
    fn transition(&mut self, config: &Config, active: bool, input: &Inputs) {
        let cs = &input.car;
        let stop = input.longitudinal.stop;
        let starting = !stop && !cs.cruise_standstill && !cs.brake;
        self.state = if !active {
            State::Off
        } else {
            match self.state {
                State::Off if !starting => State::Stopping,
                State::Off | State::Stopping if starting => {
                    if config.starting_state {
                        State::Starting
                    } else {
                        State::Pid
                    }
                }
                State::Starting | State::Pid if stop => {
                    if cs.accel > self.stopping_accel
                        || (input.radar.lead.status && input.radar.lead.distance < 4.)
                        || self.state == State::Starting
                    {
                        State::Stopping
                    } else {
                        self.state
                    }
                }
                State::Starting | State::Pid if cs.speed > config.starting_speed => State::Pid,
                other => other,
            }
        };
        if active && cs.soft_hold {
            self.state = State::Stopping;
        }
    }
    pub fn update(
        &mut self,
        config: &Config,
        params: &mut impl Parameters,
        active: bool,
        input: &Inputs,
        limits: [f64; 2],
    ) -> Result<[f64; 3], Error> {
        self.reads += 1;
        if self.reads >= 100 {
            self.reads = 0;
            self.refresh_stop(params)?;
        } else if self.reads == 10 {
            if config.brand == "hyundai" {
                self.pid.gains = Gains::constants(1., 0., 1.);
            } else if config.long_gains.p.x.len() == 1 && config.long_gains.i.x.len() == 1 {
                self.pid.gains.p.y = vec![params.float("LongTuningKpV")? * 0.01];
                self.pid.gains.i.y = vec![params.float("LongTuningKiV")? * 0.001];
                self.pid.gains.f = params.float("LongTuningKf")? * 0.01;
            }
        }
        self.pid.low = limits[0];
        self.pid.high = limits[1];
        self.transition(config, active, input);
        let plan = &input.longitudinal;
        let output = match self.state {
            State::Off => {
                self.reset();
                0.
            }
            State::Stopping => {
                let mut output = if input.car.soft_hold {
                    config.stop_accel
                } else {
                    self.last
                };
                if output > self.stopping_accel {
                    output = minimum(output, 0.) - config.stop_rate * 0.01;
                }
                self.reset();
                output
            }
            State::Starting => {
                self.reset();
                config.start_accel
            }
            State::Pid => {
                let relief = eligible_relief(config, input, limits[1]);
                if relief == 0. {
                    self.correction = 0.;
                }
                let error = if config.brand == "toyota" {
                    plan.acceleration - input.car.accel
                } else {
                    plan.speed - input.car.speed
                };
                let mut step = Step::new(error, input.car.speed, plan.acceleration);
                let previous = self.pid.i;
                let mut output = self.pid.update(step)?;
                if relief > 0. && output < 0. {
                    self.pid.i = previous;
                    step.freeze = true;
                    output = self.pid.update(step)?;
                    self.correction =
                        minimum(-output * minimum(1., relief), self.correction + 0.5 * 0.01);
                    output = minimum(0., output + self.correction);
                } else {
                    self.correction = 0.;
                }
                output
            }
        };
        self.last = clip(output, limits[0], limits[1]);
        Ok([self.last, plan.acceleration, plan.jerk])
    }
}
fn eligible_relief(config: &Config, input: &Inputs, upper: f64) -> f64 {
    let p = &input.longitudinal;
    let cs = &input.car;
    let radar = &input.radar;
    if p.coast_target > 0.
        && p.coast_percent > 0
        && config.openpilot_long
        && (0. ..=0.2).contains(&input.plan_age)
        && p.cruise_source
        && !p.fcw
        && !p.stop
        && !cs.brake
        && !cs.gas
        && !cs.carrot_cruise
        && (p.cruise_target - cs.cruise).abs() < 0.001
        && !cs.cruise_standstill
        && !radar.lead.status
        && !radar.lead_two
        && !radar.cut_in
        && cs.accel.is_finite()
        && p.acceleration.is_finite()
        && p.speed.is_finite()
        && upper >= 0.
    {
        relief(cs.speed, p.coast_target, f64::from(p.coast_percent))
    } else {
        0.
    }
}
pub fn relief(speed: f64, target: f64, percent: f64) -> f64 {
    if !(speed.is_finite()
        && target.is_finite()
        && percent.is_finite()
        && target > 10. / 3.6
        && percent > 0.
        && percent <= 10.)
    {
        return 0.;
    }
    let progress = (speed - target) / (target * percent / 100.);
    fn smooth(x: f64) -> f64 {
        let x = clip(x, 0., 1.);
        x * x * (3. - 2. * x)
    }
    smooth(progress / 0.1) * (1. - smooth((progress - 0.6) / 0.4))
}
