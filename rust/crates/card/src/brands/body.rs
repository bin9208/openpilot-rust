use crate::{
    core::{ApplyInput, ApplyOutput, Error, Message, Vehicle},
    firmware::Firmware,
    firmware_query::StartupIo,
    state_helpers::SpeedFilter,
    vehicle_params::{self, FinishOptions},
};
use num_traits::ToPrimitive;
use openpilot_can::{dbc::Dbc, packer::Packer, parser::Parser, Frame, Packet};
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use openpilot_control_policy::{
    math::clip,
    pid::{Gains, Pid, Step},
};
use openpilot_params::Params;
use std::{path::Path, sync::Arc};

const SPEED_FROM_RPM: f64 = 0.008587;

pub fn parameters(
    candidate: &str,
    firmware: &[Firmware],
    settings: &Params,
) -> Result<Message, Error> {
    let mut message = vehicle_params::baseline(candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_not_car(true);
    cp.set_brand("body");
    cp.reborrow()
        .init_safety_configs(1)
        .get(0)
        .set_safety_model(car_params::SafetyModel::Body);
    cp.set_min_steer_speed(f32::NEG_INFINITY);
    cp.set_max_lateral_accel(f32::INFINITY);
    cp.set_steer_limit_timer(1.);
    cp.set_steer_actuator_delay(0.);
    cp.set_wheel_speed_factor(SPEED_FROM_RPM as f32);
    cp.set_radar_unavailable(true);
    cp.set_openpilot_longitudinal_control(true);
    cp.set_steer_control_type(car_params::SteerControlType::Angle);
    vehicle_params::finish(cp, settings, FinishOptions { firmware })?;
    Ok(message)
}

pub struct Body {
    parser: Parser,
    packer: Packer,
    speed: SpeedFilter,
    out: Message,
    factor: f64,
    speed_pid: Pid,
    turn_pid: Pid,
    left_filtered: f64,
    right_filtered: f64,
    cluster_seen: bool,
}
impl Body {
    pub fn new(params: &[u8], dbc_root: &Path, now_ns: u64) -> Result<Self, Error> {
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(params),
            capnp::message::ReaderOptions::new(),
        )?;
        let factor = f64::from(
            reader
                .get_root::<car_params::Reader>()?
                .get_wheel_speed_factor(),
        );
        let dbc = Arc::new(Dbc::load(&dbc_root.join("comma_body.dbc"))?);
        let mut out = Message::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            parser: Parser::new(Arc::clone(&dbc), 0, now_ns),
            packer: Packer::new(dbc),
            speed: SpeedFilter::new()?,
            out,
            factor,
            speed_pid: Pid::new(Gains::constants(110., 11.5, 0.), [-1e308, 1e308]),
            turn_pid: Pid::new(Gains::constants(110., 11.5, 0.), [-1e308, 1e308]),
            left_filtered: 0.,
            right_filtered: 0.,
            cluster_seen: false,
        })
    }
}

impl Vehicle for Body {
    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.parser.diagnostics)
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }
    fn update(&mut self, packets: &[Packet], now_ns: u64) -> Result<Message, Error> {
        self.parser.update(packets)?;
        let left = self
            .parser
            .signal_lazy("MOTORS_DATA", "SPEED_L", now_ns)?
            .to_f32()
            .ok_or(Error::Numeric)?;
        let right = self
            .parser
            .signal_lazy("MOTORS_DATA", "SPEED_R", now_ns)?
            .to_f32()
            .ok_or(Error::Numeric)?;
        let raw = ((f64::from(left) + f64::from(right)) / 2. * self.factor)
            .to_f32()
            .ok_or(Error::Numeric)?;
        let [speed, accel] = self.speed.update(f64::from(raw));
        let mut message = Message::new_default();
        let mut cs = message.init_root::<car_state::Builder>();
        let mut wheels = cs.reborrow().init_wheel_speeds();
        wheels.set_fl(left);
        wheels.set_fr(right);
        cs.set_v_ego_raw(raw);
        cs.set_v_ego(speed.to_f32().ok_or(Error::Numeric)?);
        cs.set_a_ego(accel.to_f32().ok_or(Error::Numeric)?);
        cs.set_standstill(false);
        let mut fault = false;
        for signal in ["MOTOR_ERR_L", "MOTOR_ERR_R", "FAULT"] {
            fault |= self.parser.signal_lazy("VAR_VALUES", signal, now_ns)? != 0.;
        }
        cs.set_steer_fault_permanent(fault);
        cs.set_charging(
            self.parser
                .signal_lazy("BODY_DATA", "CHARGER_CONNECTED", now_ns)?
                == 1.,
        );
        cs.set_fuel_gauge(
            (self
                .parser
                .signal_lazy("BODY_DATA", "BATT_PERCENTAGE", now_ns)?
                / 100.)
                .to_f32()
                .ok_or(Error::Numeric)?,
        );
        cs.set_gear_shifter(car_state::GearShifter::Drive);
        let mut cruise = cs.reborrow().init_cruise_state();
        cruise.set_enabled(true);
        cruise.set_available(true);
        cs.set_can_valid(self.parser.can_valid());
        cs.set_can_timeout(self.parser.bus_timeout());
        let speed = cs.reborrow_as_reader().get_v_ego();
        if !self.cluster_seen {
            cs.set_v_ego_cluster(speed);
        } else {
            self.cluster_seen = true;
        }
        self.out.set_root(cs.into_reader())?;
        Ok(message)
    }
    fn init(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        Ok(())
    }
    fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let mut left = 0;
        let mut right = 0;
        let actuators = input.control.get_actuators()?;
        if input.control.get_enabled() {
            let cs = self.out.get_root_as_reader::<car_state::Reader>()?;
            let wheels = cs.get_wheel_speeds()?;
            let measured =
                SPEED_FROM_RPM * (f64::from(wheels.get_fl()) + f64::from(wheels.get_fr())) / 2.;
            let torque = self.speed_pid.update(Step::new(
                f64::from(actuators.get_accel()) / 5. - measured,
                0.,
                0.,
            ))?;
            let measured =
                SPEED_FROM_RPM * (f64::from(wheels.get_fl()) - f64::from(wheels.get_fr()));
            let error = measured - (-f64::from(actuators.get_torque()) / 2.);
            let integral = self.turn_pid.i / 11.5;
            let mut step = Step::new(error, 0., 0.);
            step.freeze = (error < 0. && integral <= -0.1) || (error > 0. && integral >= 0.1);
            let difference = self.turn_pid.update(step)?;
            let deadband = |torque: f64| {
                if torque > 0. {
                    torque + 10.
                } else {
                    torque - 10.
                }
            };
            self.right_filtered = clip(
                deadband(torque + difference),
                self.right_filtered - 50.,
                self.right_filtered + 50.,
            );
            self.left_filtered = clip(
                deadband(torque - difference),
                self.left_filtered - 50.,
                self.left_filtered + 50.,
            );
            right = clip(self.right_filtered, -500., 500.)
                .to_i32()
                .ok_or(Error::Numeric)?;
            left = clip(self.left_filtered, -500., 500.)
                .to_i32()
                .ok_or(Error::Numeric)?;
        }
        let address = self.packer.dbc.message("TORQUE_CMD")?.address;
        let data = self.packer.pack(
            "TORQUE_CMD",
            &[
                ("TORQUE_L", f64::from(left)),
                ("TORQUE_R", f64::from(right)),
            ],
            None,
        )?;
        let mut output = Message::new_default();
        output.set_root(actuators)?;
        let mut result = output.get_root::<car_control::actuators::Builder>()?;
        result.set_accel(left as f32);
        result.set_torque(right as f32);
        result.set_torque_output_can(right as f32);
        Ok(ApplyOutput {
            actuators: output,
            can: vec![Frame {
                address,
                data,
                bus: 0,
            }],
        })
    }
    fn commit_state(&mut self, state: car_state::Reader<'_>) -> Result<(), Error> {
        self.out.set_root(state)?;
        Ok(())
    }
    fn set_soft_hold(&mut self, _: i16) {}
}
