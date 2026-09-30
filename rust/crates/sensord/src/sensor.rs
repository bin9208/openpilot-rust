use crate::{Bus, Clock, Error};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Accelerometer,
    Gyroscope,
    TemperatureSensor,
}
impl Kind {
    pub fn service(self) -> &'static str {
        match self {
            Self::Accelerometer => "accelerometer",
            Self::Gyroscope => "gyroscope",
            Self::TemperatureSensor => "temperatureSensor",
        }
    }
    pub fn interrupt(self) -> bool {
        !matches!(self, Self::TemperatureSensor)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Velodyne,
    Lsm6ds3,
    Lsm6ds3trc,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Value {
    Acceleration([f32; 3]),
    GyroUncalibrated([f32; 3]),
    Temperature(f32),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: i64,
    pub source: Source,
    #[serde(flatten)]
    pub value: Value,
}
pub struct Sensor<B> {
    pub kind: Kind,
    pub bus: B,
    pub source: Source,
    start: f64,
}
impl<B: Bus> Sensor<B> {
    pub fn new(kind: Kind, bus: B) -> Self {
        Self {
            kind,
            bus,
            source: Source::Velodyne,
            start: 0.,
        }
    }
    pub(crate) fn byte(&mut self, reg: u8) -> Result<u8, Error> {
        self.bus
            .read(reg, 1)?
            .first()
            .copied()
            .ok_or(Error::Contract("short sensor read"))
    }
    pub fn reset(&mut self, clock: &mut impl Clock) -> Result<(), Error> {
        if self.kind.interrupt() {
            self.bus.write(0x12, 1)?;
            clock.sleep(0.1);
        }
        Ok(())
    }
    pub fn init(&mut self, clock: &mut impl Clock, self_test: Option<&str>) -> Result<(), Error> {
        self.source = match self.byte(0x0f)? {
            0x69 => Source::Lsm6ds3,
            0x6a => Source::Lsm6ds3trc,
            _ => return Err(Error::Contract("unexpected sensor chip ID")),
        };
        match self.kind {
            Kind::Accelerometer => {
                if self_test == Some("1") {
                    self.self_test(clock, 1)?;
                    self.self_test(clock, 2)?;
                }
                let int1 = self.byte(0x0d)? | 1;
                for (reg, value) in [(0x12, 4), (0x10, 0x40), (0x0b, 0x80), (0x0d, int1)] {
                    self.bus.write(reg, value)?;
                }
            }
            Kind::Gyroscope => {
                if self_test.is_some() {
                    self.self_test(clock, 4)?;
                    self.self_test(clock, 12)?;
                }
                self.bus.write(0x11, 0x40)?;
                self.bus.write(0x0b, 0x80)?;
                let int1 = self.byte(0x0d)? | 2;
                self.bus.write(0x0d, int1)?;
            }
            Kind::TemperatureSensor => {}
        }
        Ok(())
    }
    pub fn get_event(
        &mut self,
        clock: &mut impl Clock,
        timestamp: Option<i128>,
    ) -> Result<Event, Error> {
        let (timestamp, value) = match self.kind {
            Kind::Accelerometer | Kind::Gyroscope => {
                let timestamp = timestamp.ok_or(Error::Contract("IRQ timestamp required"))?;
                let bit = if self.kind == Kind::Accelerometer {
                    1
                } else {
                    2
                };
                if self.byte(0x1e)? & bit == 0 {
                    return Err(Error::DataNotReady);
                }
                let raw = self.raw_vector()?;
                let scale = if self.kind == Kind::Accelerometer {
                    9.81 * 2. / 32768.
                } else {
                    (8.75 / 1000.) * (std::f64::consts::PI / 180.)
                };
                let values = [
                    (f64::from(raw[1]) * scale) as f32,
                    (-f64::from(raw[0]) * scale) as f32,
                    (f64::from(raw[2]) * scale) as f32,
                ];
                (
                    timestamp,
                    if self.kind == Kind::Accelerometer {
                        Value::Acceleration(values)
                    } else {
                        Value::GyroUncalibrated(values)
                    },
                )
            }
            Kind::TemperatureSensor => {
                let ns = (clock.monotonic() * 1e9).trunc();
                if !(-9223372036854775808.0..9223372036854775808.0).contains(&ns) {
                    return Err(Error::Contract("sensor timestamp out of range"));
                }
                let bytes = self.bus.read(0x20, 2)?;
                let bytes: &[u8; 2] = bytes
                    .get(..2)
                    .and_then(|v| v.try_into().ok())
                    .ok_or(Error::Contract("short sensor read"))?;
                let scale = if self.source == Source::Lsm6ds3 {
                    16.
                } else {
                    256.
                };
                (
                    ns as i128,
                    Value::Temperature(
                        (25. + f64::from(i16::from_le_bytes(*bytes)) / scale) as f32,
                    ),
                )
            }
        };
        Ok(Event {
            timestamp: i64::try_from(timestamp)
                .map_err(|_| Error::Contract("sensor timestamp out of range"))?,
            source: self.source,
            value,
        })
    }
    pub fn valid(&mut self, clock: &mut impl Clock) -> bool {
        if self.start == 0. {
            self.start = clock.monotonic();
        }
        clock.monotonic() - self.start > 0.5
    }
    pub(crate) fn raw_vector(&mut self) -> Result<[i16; 3], Error> {
        let reg = if self.kind == Kind::Accelerometer {
            0x28
        } else {
            0x22
        };
        let data = self.bus.read(reg, 6)?;
        let bytes: &[u8; 6] = data
            .get(..6)
            .and_then(|v| v.try_into().ok())
            .ok_or(Error::Contract("short sensor read"))?;
        Ok([
            i16::from_le_bytes([bytes[0], bytes[1]]),
            i16::from_le_bytes([bytes[2], bytes[3]]),
            i16::from_le_bytes([bytes[4], bytes[5]]),
        ])
    }
    pub fn shutdown(&mut self) -> Result<(), Error> {
        match self.kind {
            Kind::Accelerometer | Kind::Gyroscope => {
                let (bit, control) = if self.kind == Kind::Accelerometer {
                    (1, 0x10)
                } else {
                    (2, 0x11)
                };
                let int1 = self.byte(0x0d)? & !bit;
                self.bus.write(0x0d, int1)?;
                let odr = self.byte(control)? & 15;
                self.bus.write(control, odr)?;
            }
            Kind::TemperatureSensor => {}
        }
        Ok(())
    }
}
pub fn parse_16bit(lsb: u8, msb: u8) -> i16 {
    i16::from_le_bytes([lsb, msb])
}
pub fn parse_20bit(b2: u8, b1: u8, b0: u8) -> i32 {
    ((i32::from(b0) << 16) | (i32::from(b1) << 8) | i32::from(b2)) / 16
}
