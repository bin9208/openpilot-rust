use openpilot_control_policy::{pid::Gains, vehicle::Physical};
use openpilot_controlsd::{
    config::{Config, Tuning},
    inputs::Inputs,
    longitudinal::{Longitudinal, State},
    parameters::Parameters,
    suspend::Suspend,
    Error,
};
struct Params {
    stop: f64,
    invalid: bool,
    keys: Vec<&'static str>,
}
impl Parameters for Params {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        self.keys.push(key);
        Ok(45)
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        self.keys.push(key);
        if self.invalid {
            Err(Error::Contract("invalid native number"))
        } else {
            Ok(self.stop)
        }
    }
    fn boolean(&mut self, _: &'static str) -> Result<bool, Error> {
        unreachable!()
    }
    fn string(&mut self, _: &'static str) -> Result<Option<String>, Error> {
        unreachable!()
    }
    fn put_integer(&mut self, _: &'static str, _: i32) -> Result<(), Error> {
        unreachable!()
    }
    fn put_boolean(&mut self, _: &'static str, _: bool) -> Result<(), Error> {
        unreachable!()
    }
}
fn config() -> Config {
    Config {
        fingerprint: "HYUNDAI_PALISADE".into(),
        brand: "hyundai".into(),
        flags: 0,
        firmware: String::new(),
        physical: Physical {
            mass: 1800.,
            inertia: 3000.,
            wheelbase: 3.,
            center_front: 1.2,
            rear_ratio: 0.,
            stiffness_front: 80000.,
            stiffness_rear: 90000.,
            steer_ratio: 15.,
        },
        angle: false,
        tuning: Tuning::Other,
        long_gains: Gains::constants(1., 0., 1.),
        min_steer_speed: 0.,
        standstill_steering: false,
        saturation_time: 0.8,
        starting_state: true,
        starting_speed: 0.5,
        stop_accel: -2.,
        start_accel: 1.,
        stop_rate: 0.8,
        openpilot_long: true,
        pcm_cruise: false,
        steer_delay: 0.2,
        bus_offset: 0,
    }
}
#[test]
fn stopping_ramp_does_not_unwind_stronger_braking() {
    let config = config();
    let mut params = Params {
        stop: -50.,
        invalid: false,
        keys: Vec::new(),
    };
    let mut controller = Longitudinal::new(&config, &mut params).unwrap();
    let mut input = Inputs::default();
    input.longitudinal.stop = true;
    controller.last = -1.2;
    assert_eq!(
        controller
            .update(&config, &mut params, true, &input, [-4., 2.])
            .unwrap()[0],
        -1.2
    );
    assert_eq!(controller.state, State::Stopping);
    controller.last = 0.5;
    assert_eq!(
        controller
            .update(&config, &mut params, true, &input, [-4., 2.])
            .unwrap()[0],
        -0.008
    );
    input.car.soft_hold = true;
    assert_eq!(
        controller
            .update(&config, &mut params, true, &input, [-4., 2.])
            .unwrap()[0],
        -2.
    );
    controller.reset();
    assert_eq!(controller.state, State::Stopping);
    assert_eq!(controller.last, -2.);
}
#[test]
fn stopping_parameter_fatal_conversion_and_nonfinite_are_distinct() {
    let config = config();
    let mut params = Params {
        stop: f64::NAN,
        invalid: false,
        keys: Vec::new(),
    };
    assert_eq!(
        Longitudinal::new(&config, &mut params)
            .unwrap()
            .stopping_accel,
        -0.5
    );
    params.invalid = true;
    assert!(Longitudinal::new(&config, &mut params).is_err());
}
#[test]
fn hyundai_periodic_tuning_never_reads_persisted_gains() {
    let config = config();
    let mut params = Params {
        stop: -200.,
        invalid: false,
        keys: Vec::new(),
    };
    let mut controller = Longitudinal::new(&config, &mut params).unwrap();
    for _ in 0..210 {
        controller
            .update(&config, &mut params, false, &Inputs::default(), [-4., 2.])
            .unwrap();
    }
    assert_eq!(controller.stopping_accel, -1.);
    assert_eq!(params.keys, vec!["StoppingAccel"; 3]);
    assert_eq!(controller.pid.gains.p.y, vec![1.]);
    assert_eq!(controller.pid.gains.i.y, vec![0.]);
    assert_eq!(controller.pid.gains.f, 1.);
}
#[test]
fn suspension_requires_full_hold_and_releases_with_hysteresis() {
    let mut params = Params {
        stop: 0.,
        invalid: false,
        keys: Vec::new(),
    };
    let mut state = Suspend::default();
    let mut input = Inputs::default();
    input.car.steering_pressed = true;
    input.car.steer_angle = 46.;
    for _ in 0..99 {
        assert!(state.update(&input.car, true, &mut params).unwrap());
    }
    assert!(!state.update(&input.car, true, &mut params).unwrap());
    input.car.steering_pressed = false;
    input.car.steer_angle = 0.;
    for _ in 0..48 {
        assert!(!state.update(&input.car, true, &mut params).unwrap());
    }
    assert!(state.update(&input.car, true, &mut params).unwrap());
}

#[test]
fn nonfinite_actuators_are_logged_before_each_field_is_zeroed() {
    use openpilot_controlsd::{controller::Command, lateral::Log, sanitize};
    let mut command = Command {
        enabled: true,
        lateral: true,
        longitudinal: true,
        left: false,
        right: false,
        accel: f32::NAN,
        target: f32::INFINITY,
        jerk: f32::NEG_INFINITY,
        curvature: f32::NAN,
        torque: f32::NAN,
        angle: f32::NAN,
        log: Log::default(),
        errors: Vec::new(),
    };
    sanitize::apply(&mut command, State::Pid).unwrap();
    assert_eq!(
        [
            command.accel,
            command.target,
            command.jerk,
            command.curvature,
            command.torque,
            command.angle
        ],
        [0.; 6]
    );
    assert_eq!(command.errors.len(), 6);
    assert!(command.errors[0].starts_with("actuators.torque not finite"));
    assert!(command.errors[1].contains("'torque': 0.0"));
    assert!(command.errors[5].contains("'jerk': 0.0, 'aTarget': inf"));
}
