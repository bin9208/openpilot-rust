use crate::{
    sensor::{Kind, Sensor, Source},
    Bus, Clock, Error,
};
impl<B: Bus> Sensor<B> {
    fn wait_ready(&mut self) -> Result<(), Error> {
        let bit = if self.kind == Kind::Accelerometer {
            1
        } else {
            2
        };
        while self.byte(0x1e)? & bit == 0 {}
        Ok(())
    }
    fn average(&mut self, scale: f64) -> Result<[f64; 3], Error> {
        let mut out = [0.; 3];
        for _ in 0..5 {
            self.wait_ready()?;
            let values = self.raw_vector()?;
            for (j, value) in values.into_iter().enumerate() {
                out[j] += f64::from(value) * scale;
            }
        }
        Ok(out.map(|v| v / 5.))
    }
    pub fn self_test(&mut self, clock: &mut impl Clock, test: u8) -> Result<(), Error> {
        let (control, scale, off_delay, on_delay, min, max, label) = match self.kind {
            Kind::Accelerometer => {
                self.bus.write(0x12, 0x44)?;
                let (odr, scale) = if self.source == Source::Lsm6ds3trc {
                    (0x38, 0.122)
                } else {
                    (0x30, 0.061)
                };
                self.bus.write(0x10, odr)?;
                (0x10, scale, 0.1, 0.1, 90., 1700., "Accelerometer")
            }
            Kind::Gyroscope => {
                self.bus.write(0x11, 0x5c)?;
                (0x11, 70., 0.15, 0.05, 150000., 700000., "Gyroscope")
            }
            Kind::TemperatureSensor => return Err(Error::Contract("temperature has no self-test")),
        };
        clock.sleep(off_delay);
        self.wait_ready()?;
        let off = self.average(scale)?;
        self.bus.write(0x14, test)?;
        clock.sleep(on_delay);
        self.wait_ready()?;
        let on = self.average(scale)?;
        self.bus.write(control, 0)?;
        self.bus.write(0x14, 0)?;
        for (a, b) in on.into_iter().zip(off) {
            let difference = (a - b).abs();
            if difference < min || difference > max {
                return Err(Error::Sensor(format!(
                    "{label} self-test failed for test type {test}"
                )));
            }
        }
        Ok(())
    }
}
