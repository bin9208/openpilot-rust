use openpilot_pandad::firmware::{
    dfu_serial, dfu_usb::DfuUsb, flash_static, Error, Mcu, Request, Transport,
};
use std::{collections::VecDeque, convert::Infallible};

#[derive(Default)]
struct Recorder {
    reads: VecDeque<Vec<u8>>,
    controls: Vec<(Request, Vec<u8>)>,
    writes: Vec<(u8, Vec<u8>)>,
}

impl Transport for Recorder {
    type Error = Infallible;
    fn control_read(&mut self, request: Request, length: usize) -> Result<Vec<u8>, Self::Error> {
        self.controls.push((request, length.to_le_bytes().to_vec()));
        Ok(self.reads.pop_front().unwrap())
    }
    fn control_write(&mut self, request: Request, data: &[u8]) -> Result<(), Self::Error> {
        self.controls.push((request, data.to_vec()));
        Ok(())
    }
    fn bulk_write(
        &mut self,
        endpoint: u8,
        data: &[u8],
        timeout_ms: u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(timeout_ms, 15_000);
        self.writes.push((endpoint, data.to_vec()));
        Ok(())
    }
}

fn flasher() -> Recorder {
    Recorder {
        reads: VecDeque::from([vec![0, 0, 0, 0, 0xde, 0xad, 0xd0, 0x0d]]),
        ..Recorder::default()
    }
}

#[test]
fn exact_sector_boundary_erases_the_next_sector_before_writing() {
    let code = vec![17; 0x4000];
    let mut recorder = flasher();
    flash_static(&mut recorder, &code, Mcu::F4).unwrap();
    let erase: Vec<_> = recorder
        .controls
        .iter()
        .filter(|(request, _)| request.request == 0xb2)
        .collect();
    assert_eq!(
        erase
            .iter()
            .map(|(request, _)| request.value)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(recorder.writes.len(), 1024);
    assert_eq!(
        recorder
            .writes
            .iter()
            .flat_map(|(endpoint, bytes)| {
                assert_eq!(*endpoint, 2);
                bytes.iter().copied()
            })
            .collect::<Vec<_>>(),
        code
    );
    assert!(recorder.controls.last().unwrap().0.expect_disconnect);
}

#[test]
fn provisioning_sector_is_rejected_before_unlock_or_erase() {
    let mut recorder = flasher();
    assert!(matches!(
        flash_static(&mut recorder, &vec![0; 6 * 0x20000], Mcu::H7),
        Err(Error::NoSector)
    ));
    assert_eq!(recorder.controls.len(), 1);
    assert!(recorder.writes.is_empty());
}

#[test]
fn invalid_flasher_never_erases_or_writes() {
    let mut recorder = Recorder {
        reads: VecDeque::from([vec![0; 12]]),
        ..Recorder::default()
    };
    assert!(matches!(
        flash_static(&mut recorder, &[1, 2, 3], Mcu::F4),
        Err(Error::FlasherMissing)
    ));
    assert_eq!(recorder.controls.len(), 1);
}

#[test]
fn dfu_identity_preserves_source_overflow_rejection_and_mcu_offset() {
    assert_eq!(
        dfu_serial("010002000300040005000600", Mcu::F4).unwrap(),
        Some("000800100004".to_owned())
    );
    assert_eq!(
        dfu_serial("010002000300040005000600", Mcu::H7).unwrap(),
        Some("000800060004".to_owned())
    );
    assert_eq!(
        dfu_serial("FFFFFFFFFFFFFFFFFFFFFFFF", Mcu::H7).unwrap(),
        None
    );
    assert_eq!(dfu_serial("none", Mcu::H7).unwrap(), None);
    assert!(dfu_serial("not hex", Mcu::F4).is_err());
}

#[test]
fn usb_dfu_pads_only_the_final_block_and_keeps_status_timeout_disabled() {
    let mut recorder = Recorder {
        reads: VecDeque::from(vec![vec![0; 6]; 4]),
        ..Recorder::default()
    };
    DfuUsb {
        transport: &mut recorder,
        mcu: Mcu::H7,
    }
    .program(0x0800_0000, &vec![0xa5; 2049])
    .unwrap();
    let blocks: Vec<_> = recorder
        .controls
        .iter()
        .filter(|(request, _)| request.request == 1 && request.value >= 2)
        .collect();
    assert_eq!(
        blocks
            .iter()
            .map(|(request, _)| request.value)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(blocks[0].1, vec![0xa5; 1024]);
    assert_eq!(blocks[1].1, vec![0xa5; 1024]);
    assert_eq!(blocks[2].1[0], 0xa5);
    assert_eq!(blocks[2].1[1..], [0xff; 1023]);
    assert!(recorder
        .controls
        .iter()
        .all(|(request, _)| request.timeout_ms == 0));
}

#[test]
fn empty_usb_dfu_image_stops_after_setting_the_address() {
    let mut recorder = Recorder {
        reads: VecDeque::from([vec![0; 6]]),
        ..Recorder::default()
    };
    let result = DfuUsb {
        transport: &mut recorder,
        mcu: Mcu::F4,
    }
    .program(0x0800_0000, &[]);
    assert!(matches!(result, Err(Error::EmptyBinary)));
    assert_eq!(recorder.controls.len(), 2);
    assert_eq!(recorder.controls[0].1, [0x21, 0, 0, 0, 8]);
    assert!(recorder.writes.is_empty());
}
