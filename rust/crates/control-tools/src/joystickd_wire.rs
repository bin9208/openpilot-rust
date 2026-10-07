use crate::{
    joystickd::{Command, Config, Input, LongState},
    Error,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::{
    car_capnp::{car_control::actuators::LongControlState, car_params},
    log_capnp::event,
};
use openpilot_control_policy::vehicle::Physical;
use openpilot_messaging::state::State;

pub const TOPICS: [&str; 5] = [
    "carState",
    "onroadEvents",
    "liveParameters",
    "selfdriveState",
    "testJoystick",
];

pub fn config(bytes: &[u8]) -> Result<Config, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let cp = message.get_root::<car_params::Reader<'_>>()?;
    Ok(Config {
        physical: Physical {
            mass: f64::from(cp.get_mass()),
            inertia: f64::from(cp.get_rotational_inertia()),
            wheelbase: f64::from(cp.get_wheelbase()),
            center_front: f64::from(cp.get_center_to_front()),
            rear_ratio: f64::from(cp.get_steer_ratio_rear()),
            stiffness_front: f64::from(cp.get_tire_stiffness_front()),
            stiffness_rear: f64::from(cp.get_tire_stiffness_rear()),
            steer_ratio: f64::from(cp.get_steer_ratio()),
        },
        stopping_speed: f64::from(cp.get_v_ego_stopping()),
        openpilot_longitudinal: cp.get_openpilot_longitudinal_control(),
        pcm_cruise: cp.get_pcm_cruise(),
    })
}

pub fn input(state: &State) -> Result<Input, Error> {
    let event::Which::CarState(car) = state
        .topic("carState")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid carState union"))?
    else {
        return Err(Error::Contract("unexpected carState event"));
    };
    let car = car?;
    let event::Which::SelfdriveState(drive) = state
        .topic("selfdriveState")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid selfdriveState union"))?
    else {
        return Err(Error::Contract("unexpected selfdriveState event"));
    };
    let drive = drive?;
    let event::Which::LiveParameters(live) = state
        .topic("liveParameters")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid liveParameters union"))?
    else {
        return Err(Error::Contract("unexpected liveParameters event"));
    };
    let live = live?;
    let joystick = state.topic("testJoystick")?;
    let event::Which::TestJoystick(axes) = joystick
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid testJoystick union"))?
    else {
        return Err(Error::Contract("unexpected testJoystick event"));
    };
    let axes = axes?.get_axes()?.iter().map(f64::from).collect();
    let event::Which::OnroadEvents(events) = state
        .topic("onroadEvents")?
        .event()?
        .which()
        .map_err(|_| Error::Contract("invalid onroadEvents union"))?
    else {
        return Err(Error::Contract("unexpected onroadEvents event"));
    };
    let events = events?;
    Ok(Input {
        enabled: drive.get_enabled(),
        active: drive.get_active(),
        steer_fault_temporary: car.get_steer_fault_temporary(),
        steer_fault_permanent: car.get_steer_fault_permanent(),
        override_longitudinal: events.iter().any(|event| event.get_override_longitudinal()),
        cruise_enabled: car.get_cruise_state()?.get_enabled(),
        speed: f64::from(car.get_v_ego()),
        steering_angle: f64::from(car.get_steering_angle_deg()),
        roll: f64::from(live.get_roll()),
        angle_offset: f64::from(live.get_angle_offset_deg()),
        frame: state.frame(),
        joystick_frame: joystick.receive_frame,
        axes,
    })
}

pub fn control(command: &Command, timestamp: u64) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(timestamp);
    let mut cc = root.init_car_control();
    cc.set_enabled(command.enabled);
    cc.set_lat_active(command.lat_active);
    cc.set_long_active(command.long_active);
    {
        let mut actuators = cc.reborrow().init_actuators();
        actuators.set_accel(command.accel);
        actuators.set_torque(command.torque);
        actuators.set_steering_angle_deg(command.steering_angle);
        actuators.set_curvature(command.curvature);
        actuators.set_long_control_state(match command.long_state {
            LongState::Off => LongControlState::Off,
            LongState::Pid => LongControlState::Pid,
            LongState::Stopping => LongControlState::Stopping,
        });
    }
    {
        let mut cruise = cc.reborrow().init_cruise_control();
        cruise.set_cancel(command.cancel);
        cruise.set_resume(command.resume);
    }
    cc.init_hud_control().set_lead_distance_bars(2);
    serialize::write_message_to_words(&message)
}

pub fn controls(curvature: f32, timestamp: u64) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(true);
    root.set_log_mono_time(timestamp);
    let mut cs = root.init_controls_state();
    cs.set_curvature(curvature);
    cs.init_lateral_control_state().init_debug_state();
    serialize::write_message_to_words(&message)
}
