use crate::{
    sensor::{Kind, Sensor, Value},
    Bus, Clock, Error,
};
struct TestClock(f64);
impl Clock for TestClock {
    fn monotonic(&mut self) -> f64 {
        self.0
    }
    fn monotonic_ns(&mut self) -> i128 {
        1_000_000_000
    }
    fn realtime_ns(&mut self) -> i128 {
        2_000_000_000
    }
    fn sleep(&mut self, _: f64) {}
}
struct TestBus;
impl Bus for TestBus {
    fn read(&mut self, reg: u8, _: usize) -> Result<Vec<u8>, Error> {
        Ok(match reg {
            0x0f => vec![0x6a],
            0x1e => vec![3],
            0x28 => vec![0, 128, 0, 0, 0, 64],
            _ => vec![0],
        })
    }
    fn write(&mut self, _: u8, _: u8) -> Result<(), Error> {
        Ok(())
    }
}
#[test]
fn signed_samples_and_settling_when_native_boundaries_are_absent() {
    let mut sensor = Sensor::new(Kind::Accelerometer, TestBus);
    let mut clock = TestClock(1.);
    sensor.init(&mut clock, None).expect("chip ID");
    let event = sensor.get_event(&mut clock, Some(55)).expect("event");
    let Value::Acceleration(values) = event.value else {
        panic!("acceleration required")
    };
    assert_eq!(values, [0., 19.62, 9.81]);
    assert_eq!(event.timestamp, 55);
    assert!(!sensor.valid(&mut clock));
    clock.0 = 1.5;
    assert!(!sensor.valid(&mut clock));
    clock.0 = 1.500001;
    assert!(sensor.valid(&mut clock));
    let encoded = crate::wire::encode(Kind::Accelerometer, &event, 66).expect("encode");
    let reader =
        capnp::serialize::read_message(encoded.as_slice(), capnp::message::ReaderOptions::new())
            .expect("decode");
    let root = reader
        .get_root::<openpilot_cereal::log_capnp::event::Reader<'_>>()
        .expect("event root");
    assert!(root.get_valid());
    assert_eq!(root.get_log_mono_time(), 66);
    let openpilot_cereal::log_capnp::event::Which::Accelerometer(sensor) =
        root.which().expect("union")
    else {
        panic!("accelerometer required")
    };
    assert_eq!(sensor.expect("sensor").get_timestamp(), 55);
}
