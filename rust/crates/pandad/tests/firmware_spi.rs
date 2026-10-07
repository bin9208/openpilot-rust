use openpilot_pandad::firmware::{
    spi::{Error, Io, Mode, PandaSpi},
    Request, Transport,
};
use std::collections::VecDeque;

#[derive(Default)]
struct Fixture {
    tx: Vec<Vec<u8>>,
    replies: VecDeque<Vec<u8>>,
    fail: Option<usize>,
    clock: f64,
    locked: bool,
    auto_ack: bool,
}
impl Io for Fixture {
    fn lock(&mut self) -> Result<(), Error> {
        assert!(!self.locked);
        self.locked = true;
        Ok(())
    }
    fn unlock(&mut self) -> Result<(), Error> {
        assert!(self.locked);
        self.locked = false;
        Ok(())
    }
    fn transfer(&mut self, _: Mode, tx: &[u8]) -> Result<Vec<u8>, Error> {
        self.tx.push(tx.to_vec());
        if self.fail == Some(self.tx.len() - 1) {
            return Err(Error::Io("failed".into()));
        }
        if self.auto_ack && tx == [0] {
            return Ok(vec![0x79]);
        }
        Ok(self
            .replies
            .pop_front()
            .unwrap_or_else(|| vec![0; tx.len()]))
    }
    fn read(&mut self, _: usize) -> Result<Vec<u8>, Error> {
        unreachable!()
    }
    fn write(&mut self, _: &[u8]) -> Result<(), Error> {
        unreachable!()
    }
    fn now(&mut self) -> Result<f64, Error> {
        self.clock += 1.0 / 1024.0;
        Ok(self.clock)
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.clock += seconds;
        Ok(())
    }
    fn log(&mut self, _: String, _: Option<&Error>) -> Result<(), Error> {
        Ok(())
    }
    fn kernel(&mut self, _: u8, _: &[u8], _: usize, _: bool) -> Result<Vec<u8>, Error> {
        unreachable!()
    }
}

#[test]
fn reset_write_sends_original_control_header_then_returns_without_data_ack() {
    let io = Fixture {
        replies: VecDeque::from([vec![0; 7], vec![0x79], vec![0; 8]]),
        ..Fixture::default()
    };
    let mut spi = PandaSpi::new(io, false);
    let mut request = Request::new(0xc0, 0xd8, 1);
    request.index = 2;
    request.expect_disconnect = true;
    spi.control_write(request, &[99]).unwrap();
    assert_eq!(
        spi.io.tx,
        [
            vec![0x5a, 0, 7, 0, 0xe8, 3, 0x1d],
            vec![0x11],
            vec![0xd8, 1, 0, 2, 0, 0, 0, 0x70]
        ]
    );
    assert!(!spi.io.locked);
}

#[test]
fn os_error_releases_the_lock_without_protocol_retry() {
    let io = Fixture {
        replies: VecDeque::from([vec![0; 7], vec![0x79]]),
        fail: Some(2),
        ..Fixture::default()
    };
    let mut spi = PandaSpi::new(io, false);
    assert!(spi.control_write(Request::new(0xc0, 0xd8, 0), &[]).is_err());
    assert_eq!(spi.io.tx.len(), 3);
    assert!(!spi.io.locked);
}

#[test]
fn dfu_spi_program_pads_to_256_and_keeps_empty_images_as_noops() {
    use openpilot_pandad::firmware::{dfu_spi::DfuSpi, Mcu};
    let mut dfu = DfuSpi {
        io: Fixture {
            auto_ack: true,
            ..Fixture::default()
        },
        mcu: Mcu::H7,
    };
    dfu.program(0x0800_0000, &[]).unwrap();
    assert!(dfu.io.tx.is_empty());
    dfu.program(0x0800_0000, &[0x42]).unwrap();
    let block = dfu.io.tx.iter().find(|tx| tx.len() == 258).unwrap();
    assert_eq!(&block[..2], [255, 0x42]);
    assert!(block[2..257].iter().all(|byte| *byte == 255));
    assert_eq!(block[257], 0x42);
    assert!(!dfu.io.locked);
}
