use openpilot_usbgpu::{
    bus_lock::BusLock,
    clock::Clock,
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    usb3::Usb3,
    Error,
};
use std::{collections::VecDeque, time::Duration};
#[derive(Default)]
struct FakeClock {
    sleeps: Vec<Duration>,
}
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.sleeps.iter().sum()
    }
    fn sleep(&mut self, d: Duration) {
        self.sleeps.push(d);
    }
}
struct FakeUsb {
    outcomes: VecDeque<BulkResult>,
    calls: usize,
}
impl Transport for FakeUsb {
    fn describe(&self) -> Result<Description, Error> {
        Ok(Description {
            bus: 1,
            address: 1,
            product: b"custom".to_vec(),
        })
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn streams(&mut self, _: &[u8], _: u32) -> Result<i32, Error> {
        panic!("custom firmware must use BOT")
    }
    fn control(&mut self, _: Control, _: &mut [u8]) -> Result<i32, Error> {
        panic!("not used")
    }
    fn bulk(&mut self, _: u8, _: &mut [u8], _: u32) -> Result<BulkResult, Error> {
        self.calls += 1;
        Ok(self.outcomes.pop_front().expect("unexpected replay"))
    }
    fn batch(&mut self, _: &mut [Transfer]) -> Result<(), Error> {
        panic!("not used")
    }
    fn error_text(&self, _: i32) -> String {
        "fixture".into()
    }
}
#[test]
fn retries_only_zero_byte_eio_and_stops_after_ten() {
    let dir = tempfile::tempdir().unwrap();
    let lock = BusLock::open(&dir.path().join("lock")).unwrap();
    for (outcomes, success, calls, sleeps) in [
        (vec![(-1, 0), (-1, 0), (0, 4)], true, 3, 2),
        (vec![(-1, 2)], false, 1, 0),
        (vec![(-7, 0)], false, 1, 0),
        (vec![(0, 2)], false, 1, 0),
        (vec![(-1, 0); 10], false, 10, 9),
    ] {
        let transport = FakeUsb {
            outcomes: outcomes
                .into_iter()
                .map(|(code, actual)| BulkResult { code, actual })
                .collect(),
            calls: 0,
        };
        let mut usb = Usb3::new(transport, FakeClock::default(), lock.clone(), false).unwrap();
        assert_eq!(usb.bulk_out(2, &[1, 2, 3, 4], 1000).is_ok(), success);
        assert_eq!(usb.transport.calls, calls);
        assert_eq!(usb.clock.sleeps, vec![Duration::from_millis(10); sleeps]);
    }
}
