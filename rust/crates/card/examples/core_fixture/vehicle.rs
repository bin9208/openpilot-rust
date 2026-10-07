use super::Step;
use capnp::message::{Builder, ReaderOptions};
use openpilot_can::{Frame, Packet};
use openpilot_card::{
    core::{ApplyInput, ApplyOutput, Error, Message, StateTail, Vehicle},
    firmware_query::StartupIo,
};
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_messaging::state::State;
use serde_json::{json, Value};

pub struct Driver {
    pub step: Step,
    pub calls: Vec<Value>,
    state: Message,
    soft_hold: i16,
}
impl Driver {
    pub fn new(step: Step) -> Self {
        let mut state = Message::new_default();
        state.init_root::<car_state::Builder>();
        Self {
            step,
            calls: vec![],
            state,
            soft_hold: 0,
        }
    }
}
impl Vehicle for Driver {
    fn update(&mut self, packets: &[Packet], now_ns: u64) -> Result<Message, Error> {
        self.calls
            .push(json!({"call":"update","packets":packets,"now_ns":now_ns}));
        let reader = capnp::serialize::read_message(
            std::io::Cursor::new(&self.step.state),
            ReaderOptions::new(),
        )?;
        let mut message = Builder::new_default();
        message.set_root(reader.get_root::<car_state::Reader>()?)?;
        Ok(message)
    }
    fn init(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        self.calls.push(json!({"call":"init"}));
        Ok(())
    }
    fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        let cs = self.state.get_root_as_reader::<car_state::Reader>()?;
        self.calls.push(json!({"call":"apply","now_ns":input.now_ns,"model":input.model.is_some(),"radar":input.radar.is_some(),"soft_hold":self.soft_hold,"enabled":input.control.get_enabled(),"lat_enabled":cs.get_lat_enabled(),"activate":cs.get_activate_cruise()}));
        let mut message = Builder::new_default();
        message.set_root(input.control.get_actuators()?)?;
        message
            .get_root::<car_control::actuators::Builder>()?
            .set_accel(self.step.accel);
        Ok(ApplyOutput {
            actuators: message,
            can: vec![Frame {
                address: 123,
                data: vec![1, 2, 3],
                bus: 4,
            }],
        })
    }
    fn commit_state(&mut self, state: car_state::Reader<'_>) -> Result<(), Error> {
        self.state.set_root(state)?;
        Ok(())
    }
    fn set_soft_hold(&mut self, active: i16) {
        self.soft_hold = active;
    }
}
pub struct Tail {
    pub calls: Vec<Value>,
}
impl StateTail for Tail {
    fn update(
        &mut self,
        _: car_state::Builder<'_>,
        _: &State,
        is_metric: bool,
        _: f64,
    ) -> Result<(), Error> {
        self.calls.push(json!({"call":"tail","metric":is_metric}));
        Ok(())
    }
    fn project(&self, mut state: car_state::Builder<'_>) -> Result<(), Error> {
        state.set_log_carrot("fixture");
        state.set_v_cruise(20.);
        state.set_v_cruise_cluster(20.);
        state.set_soft_hold_active(2);
        state.set_activate_cruise(1);
        state.set_lat_enabled(true);
        state.set_use_lane_line_speed(10.);
        state.set_carrot_cruise(1);
        Ok(())
    }
    fn initialize(
        &mut self,
        previous: car_state::Reader<'_>,
        experimental: bool,
    ) -> Result<(), Error> {
        self.calls.push(
            json!({"call":"initialize","speed":previous.get_v_ego(),"experimental":experimental}),
        );
        Ok(())
    }
}
