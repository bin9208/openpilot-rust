use openpilot_pandad::device::{Control, Device, Transport};
use std::{collections::VecDeque, convert::Infallible, sync::Mutex};

struct Reply {
    request: u8,
    count: i32,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct Recorder {
    replies: Mutex<VecDeque<Reply>>,
    writes: Mutex<Vec<Control>>,
}

impl Transport for Recorder {
    type Error = Infallible;
    fn control_read(&self, command: Control, output: &mut [u8]) -> Result<i32, Self::Error> {
        let reply = self.replies.lock().unwrap().pop_front().unwrap();
        assert_eq!(command.request, reply.request);
        assert_eq!(command.timeout_ms, 0);
        output[..reply.bytes.len()].copy_from_slice(&reply.bytes);
        Ok(reply.count)
    }
    fn control_write(&self, command: Control) -> Result<i32, Self::Error> {
        self.writes.lock().unwrap().push(command);
        Ok(0)
    }
}

fn device(replies: Vec<Reply>) -> Device<Recorder> {
    let recorder = Recorder::default();
    recorder.replies.lock().unwrap().push_back(Reply {
        request: 0xc1,
        count: 1,
        bytes: vec![6],
    });
    recorder.replies.lock().unwrap().extend(replies);
    Device::connect(recorder, 4).unwrap()
}

#[test]
fn short_successful_health_read_keeps_zero_initialized_source_tail() {
    let device = device(vec![Reply {
        request: 0xd2,
        count: 5,
        bytes: vec![1, 2, 3, 4, 5],
    }]);
    let health = device.health().unwrap().unwrap();
    assert_eq!(health.uptime, 0x04030201);
    assert_eq!(health.voltage, 5);
    assert_eq!(health.som_reset_triggered, 0);
    assert_eq!(device.hardware_type(), 6);
    assert_eq!(
        device.transport().writes.lock().unwrap().as_slice(),
        [Control::new(0xc0, 0, 0)]
    );
}

#[test]
fn negative_health_read_is_missing_instead_of_zero_health() {
    let device = device(vec![Reply {
        request: 0xd2,
        count: -4,
        bytes: vec![],
    }]);
    assert!(device.health().unwrap().is_none());
}

#[test]
fn firmware_signature_requires_both_complete_reads_and_still_issues_the_second() {
    let device = device(vec![
        Reply {
            request: 0xd3,
            count: 63,
            bytes: vec![1; 63],
        },
        Reply {
            request: 0xd4,
            count: 64,
            bytes: vec![2; 64],
        },
    ]);
    assert!(device.firmware_signature().unwrap().is_none());
    assert!(device.transport().replies.lock().unwrap().is_empty());
}

#[test]
fn serial_stream_retains_embedded_nul_bytes_and_stops_on_error() {
    let device = device(vec![
        Reply {
            request: 0xe0,
            count: 3,
            bytes: vec![65, 0, 66],
        },
        Reply {
            request: 0xe0,
            count: -4,
            bytes: vec![],
        },
    ]);
    assert_eq!(device.serial_read(0).unwrap(), [65, 0, 66]);
}

#[test]
fn configuration_reads_firmware_even_when_check_is_skipped() {
    let device = device(vec![
        Reply {
            request: 0xd3,
            count: -1,
            bytes: vec![],
        },
        Reply {
            request: 0xd4,
            count: -1,
            bytes: vec![],
        },
    ]);
    device
        .configure(true, true, |_| {
            panic!("missing signature cannot read firmware files")
        })
        .unwrap();
    assert!(device.transport().replies.lock().unwrap().is_empty());
    assert_eq!(
        device.transport().writes.lock().unwrap().as_slice(),
        [
            Control::new(0xc0, 0, 0),
            Control::new(0xe5, 1, 0),
            Control::new(0xe8, 0, 1),
            Control::new(0xe8, 1, 1),
            Control::new(0xe8, 2, 1),
        ]
    );
}

#[test]
fn firmware_matching_uses_tail_signature_and_stops_at_the_first_match() {
    let device = device(vec![
        Reply {
            request: 0xd3,
            count: 64,
            bytes: vec![1; 64],
        },
        Reply {
            request: 0xd4,
            count: 64,
            bytes: vec![2; 64],
        },
    ]);
    let mut paths = Vec::new();
    device
        .configure(false, false, |path| {
            paths.push(path.to_owned());
            if path == "../../../panda/board/obj/panda_h7.bin.signed" {
                [vec![99; 321], vec![1; 64], vec![2; 64]].concat()
            } else {
                vec![0; 128]
            }
        })
        .unwrap();
    assert_eq!(
        paths,
        [
            "../../../panda/board/obj/panda.bin.signed",
            "../../../panda/board/obj/panda_h7.bin.signed"
        ]
    );
}

#[test]
fn firmware_mismatch_is_fatal_after_all_source_paths_are_checked() {
    let device = device(vec![
        Reply {
            request: 0xd3,
            count: 64,
            bytes: vec![1; 64],
        },
        Reply {
            request: 0xd4,
            count: 64,
            bytes: vec![2; 64],
        },
    ]);
    let mut paths = Vec::new();
    let error = device
        .configure(false, false, |path| {
            paths.push(path.to_owned());
            vec![0; 127]
        })
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Panda firmware out of date. Run pandad.py to update."
    );
    assert_eq!(
        paths,
        [
            "../../../panda/board/obj/panda.bin.signed",
            "../../../panda/board/obj/panda_h7.bin.signed",
            "../../panda/board/obj/panda.bin.signed",
            "../../panda/board/obj/panda_h7.bin.signed",
        ]
    );
}
