use capnp::message::{Builder, HeapAllocator};
use openpilot_can::{Frame, Packet};
use openpilot_card::{
    core::{ApplyInput, ApplyOutput, Card, Error, StateTail, StepIo, Vehicle, VehicleLog},
    firmware_query::StartupIo,
    isotp,
    query::QueryIo,
};
use openpilot_cereal::{
    car_capnp::{car_params, car_state},
    log_capnp::event,
};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;

struct Driver {
    calls: Vec<&'static str>,
    logs: Vec<VehicleLog>,
    writes: Vec<(String, Vec<u8>)>,
    queue_write: bool,
    fail_apply: bool,
}
impl Vehicle for Driver {
    fn take_param_writes(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.writes)
    }
    fn take_logs(&mut self) -> Vec<VehicleLog> {
        std::mem::take(&mut self.logs)
    }
    fn update(&mut self, _: &[Packet], _: u64) -> Result<Builder<HeapAllocator>, Error> {
        self.calls.push("update");
        let mut m = Builder::new_default();
        m.init_root::<car_state::Builder>().set_can_valid(true);
        Ok(m)
    }
    fn init(&mut self, _: &mut impl StartupIo) -> Result<(), Error> {
        self.calls.push("init");
        Ok(())
    }
    fn apply(&mut self, input: ApplyInput<'_>) -> Result<ApplyOutput, Error> {
        self.calls.push("apply");
        if self.queue_write {
            self.writes
                .push(("ActivateCruiseAfterBrake".into(), b"1".to_vec()));
        }
        if self.fail_apply {
            return Err(Error::Event("apply failure"));
        }
        let mut a = Builder::new_default();
        a.set_root(input.control.get_actuators()?)?;
        Ok(ApplyOutput {
            actuators: a,
            can: vec![],
        })
    }
    fn set_soft_hold(&mut self, _: i16) {}
    fn commit_state(&mut self, _: car_state::Reader<'_>) -> Result<(), Error> {
        Ok(())
    }
}
struct Tail {
    value: f32,
}
impl StateTail for Tail {
    fn project(&self, mut state: car_state::Builder<'_>) -> Result<(), Error> {
        state.set_v_cruise(self.value);
        Ok(())
    }
    fn update(
        &mut self,
        _: car_state::Builder<'_>,
        _: &State,
        _: bool,
        _: f64,
    ) -> Result<(), Error> {
        self.value = 1.;
        Ok(())
    }
    fn initialize(&mut self, _: car_state::Reader<'_>, _: bool) -> Result<(), Error> {
        self.value = 42.;
        Ok(())
    }
}
struct Io {
    state: State,
    sent: Vec<String>,
    now: u64,
    writes: Vec<(String, Vec<u8>)>,
    logs: Vec<(String, String)>,
    operations: Vec<String>,
    fail_vehicle_write: bool,
    writer: Option<openpilot_card::async_params::AsyncParams>,
}
impl QueryIo for Io {
    fn receive(&mut self, _: bool) -> Result<Vec<Vec<Frame>>, isotp::Error> {
        Ok(vec![])
    }
    fn send(&mut self, _: &[Frame]) -> Result<(), isotp::Error> {
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), isotp::Error> {
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now as f64 / 1e9
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, _: bool) -> Result<(), isotp::Error> {
        Ok(())
    }
}
impl StepIo for Io {
    fn vehicle_log(&mut self, diagnostic: &VehicleLog) -> Result<(), Error> {
        self.logs.push((
            format!("{:?}", diagnostic.level),
            diagnostic.message.clone(),
        ));
        Ok(())
    }
    fn put_nonblocking(&mut self, key: &str, value: &[u8]) -> Result<(), Error> {
        self.operations.push(format!("put:{key}"));
        if self.fail_vehicle_write && key == "ActivateCruiseAfterBrake" {
            return Err(Error::Event("write failure"));
        }
        if let Some(writer) = &mut self.writer {
            writer.put(key, value)?;
        }
        self.writes.push((key.to_owned(), value.to_vec()));
        Ok(())
    }
    fn receive_can_raw(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        Ok(vec![])
    }
    fn update_subscribers(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn subscribers(&self) -> &State {
        &self.state
    }
    fn monotonic_ns(&mut self) -> u64 {
        self.now += 1000;
        self.now
    }
    fn thread_cpu_ns(&mut self) -> u64 {
        self.now
    }
    fn publish(&mut self, topic: &str, bytes: &[u8]) -> Result<(), Error> {
        self.operations.push(format!("publish:{topic}"));
        self.sent.push(topic.into());
        let m = capnp::serialize::read_message(
            std::io::Cursor::new(bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let e = m.get_root::<event::Reader>()?;
        if topic == "carState" {
            let event::Which::CarState(cs) = e.which()? else {
                panic!("carState")
            };
            let state = cs?;
            assert_eq!(state.get_can_error_counter(), 1);
            assert_eq!(state.get_v_cruise(), 42.);
        }
        Ok(())
    }
    fn warning(&mut self, _: &str) -> Result<(), Error> {
        Ok(())
    }
    fn diagnostics(&mut self, _: &[(&'static str, f64)]) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn can_driven_step_publishes_previous_output_before_initializing_and_applying() {
    let root = tempfile::tempdir().unwrap();
    let settings = Params::open(root.path(), "d").unwrap();
    let mut params = Builder::new_default();
    params.init_root::<car_params::Builder>().set_passive(false);
    let mut state = State::new(&openpilot_card::core::SERVICES, Options::default()).unwrap();
    let mut cc = Builder::new_default();
    cc.init_root::<event::Builder>()
        .init_car_control()
        .set_enabled(true);
    let mut ready = Builder::new_default();
    ready.init_root::<event::Builder>().init_onroad_events(0);
    state
        .update(
            0.,
            &[
                capnp::serialize::write_message_to_words(&cc),
                capnp::serialize::write_message_to_words(&ready),
            ],
        )
        .unwrap();
    let mut io = Io {
        state,
        sent: vec![],
        now: 1,
        writes: vec![],
        logs: vec![],
        operations: vec![],
        fail_vehicle_write: false,
        writer: None,
    };
    let mut driver = Driver {
        calls: vec![],
        writes: vec![],
        queue_write: false,
        fail_apply: false,
        logs: vec![
            VehicleLog {
                level: openpilot_card::query::DiagnosticLevel::Warning,
                message: "parser warning".into(),
            },
            VehicleLog {
                level: openpilot_card::query::DiagnosticLevel::Error,
                message: "state error".into(),
            },
        ],
    };
    let mut card = Card::new(params, settings, false, true).unwrap();
    card.step(&mut driver, &mut Tail { value: 0. }, &mut io)
        .unwrap();
    assert_eq!(io.sent, ["carParams", "carOutput", "carState", "sendcan"]);
    assert_eq!(driver.calls, ["update", "init", "apply"]);
    assert_eq!(io.writes, [("ControlsReady".into(), b"1".to_vec())]);
    assert_eq!(
        io.logs,
        [
            ("Warning".into(), "parser warning".into()),
            ("Error".into(), "state error".into())
        ]
    );
}
#[path = "core/vehicle_writes.rs"]
mod vehicle_writes;
