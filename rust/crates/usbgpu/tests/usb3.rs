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

struct UasUsb {
    next: u8,
    windows: Vec<usize>,
    packets: Vec<Vec<u8>>,
}
impl Transport for UasUsb {
    fn describe(&self) -> Result<Description, Error> {
        Ok(Description {
            bus: 1,
            address: 1,
            product: b"stock".to_vec(),
        })
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn streams(&mut self, endpoints: &[u8], count: u32) -> Result<i32, Error> {
        assert_eq!(endpoints, [2, 0x81, 0x83]);
        assert_eq!(count, 93);
        Ok(93)
    }
    fn control(&mut self, _: Control, _: &mut [u8]) -> Result<i32, Error> {
        panic!("not used")
    }
    fn bulk(&mut self, _: u8, _: &mut [u8], _: u32) -> Result<BulkResult, Error> {
        panic!("UAS does not use synchronous bulk")
    }
    fn batch(&mut self, transfers: &mut [Transfer]) -> Result<(), Error> {
        self.windows
            .push(transfers.iter().filter(|t| t.endpoint == 0x81).count());
        for transfer in transfers {
            if transfer.endpoint == 4 {
                self.packets.push(transfer.data.clone());
            }
            if transfer.endpoint == 0x81 {
                transfer.data.fill(self.next);
                self.next += 1;
            }
            transfer.status = 0;
            transfer.actual = transfer.data.len() as u32;
        }
        Ok(())
    }
    fn error_text(&self, _: i32) -> String {
        "fixture".into()
    }
}
#[test]
fn uas_windows_return_each_read_once_without_stale_slots() {
    use openpilot_usbgpu::usb3::Command;
    let dir = tempfile::tempdir().unwrap();
    let lock = BusLock::open(&dir.path().join("lock")).unwrap();
    for count in [1, 30, 31, 32, 33, 62, 63] {
        let mut usb = Usb3::new(
            UasUsb {
                next: 0,
                windows: Vec::new(),
                packets: Vec::new(),
            },
            FakeClock::default(),
            lock.clone(),
            false,
        )
        .unwrap();
        let commands = (0..count)
            .map(|_| Command {
                cdb: vec![0xe4, 1, 0x50, 0, 0, 0],
                read: 1,
                write: None,
            })
            .collect::<Vec<_>>();
        let results = usb.send_batch(&commands).unwrap();
        assert_eq!(
            results,
            (0..count)
                .map(|index| Some(vec![index as u8]))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            usb.transport.windows,
            commands
                .chunks(31)
                .map(|chunk| chunk.len())
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn uas_short_cdb_retains_original_slot_template_tail() {
    use openpilot_usbgpu::usb3::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut usb = Usb3::new(
        UasUsb {
            next: 0,
            windows: Vec::new(),
            packets: Vec::new(),
        },
        FakeClock::default(),
        BusLock::open(&dir.path().join("lock")).unwrap(),
        false,
    )
    .unwrap();
    usb.send_batch(&[Command {
        cdb: (0..16).collect(),
        read: 0,
        write: None,
    }])
    .unwrap();
    usb.send_batch(&[Command {
        cdb: vec![0xe4, 1, 2, 3, 4, 5],
        read: 0,
        write: None,
    }])
    .unwrap();
    assert_eq!(&usb.transport.packets[1][16..22], &[0xe4, 1, 2, 3, 4, 5]);
    assert_eq!(
        &usb.transport.packets[1][22..32],
        &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
}
