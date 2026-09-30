use crate::Error;
use openpilot_cereal::log_capnp::event;

#[derive(Default)]
pub struct Calibration([f32; 3]);

impl Calibration {
    pub fn values(&self) -> [f32; 3] {
        self.0
    }

    pub fn update(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut &*bytes,
            capnp::message::ReaderOptions::new(),
        )?;
        let event = message.get_root::<event::Reader>()?;
        let event::Which::LiveCalibration(calibration) = event
            .which()
            .map_err(|_| Error::Contract("unknown calibration event"))?
        else {
            return Err(Error::Contract("expected liveCalibration"));
        };
        let values = calibration?.get_rpy_calib()?;
        self.0 = match values.len() {
            1 => [values.get(0); 3],
            3 => [values.get(0), values.get(1), values.get(2)],
            _ => {
                return Err(Error::Contract(
                    "calibration must broadcast to three values",
                ))
            }
        };
        Ok(())
    }
}

pub fn driver_transform(camera: [u32; 2]) -> Result<[f32; 9], Error> {
    match camera {
        [1344, 760] => Ok([0.75, 0.0, 132.0, 0.0, 0.75, 113.0, 0.0, 0.0, 1.0]),
        [1928, 1208] => Ok([1.0, 0.0, 244.0, 0.0, 1.0, 248.0, 0.0, 0.0, 1.0]),
        _ => Err(Error::Contract("unsupported driver camera dimensions")),
    }
}
