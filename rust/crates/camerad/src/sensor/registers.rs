use super::{data, Exposure, Register, SensorError, SensorKind};

#[derive(Debug)]
pub struct ExposureRegisters {
    data: [Register; 9],
    len: usize,
}

impl ExposureRegisters {
    pub fn as_slice(&self) -> &[Register] {
        &self.data[..self.len]
    }
}

impl SensorKind {
    pub fn exposure_registers(self, exposure: Exposure) -> Result<ExposureRegisters, SensorError> {
        let Exposure {
            time,
            gain_index,
            dc_gain,
        } = exposure;
        let gain_index_usize =
            usize::try_from(gain_index).map_err(|_| SensorError::GainIndex(gain_index))?;
        if self
            .config()
            .sensor_analog_gains
            .get(gain_index_usize)
            .is_none()
        {
            return Err(SensorError::GainIndex(gain_index));
        }
        let time = u32::from_ne_bytes(time.to_ne_bytes());
        let data = match self {
            Self::Ar0231 => {
                let gain =
                    u32::try_from(gain_index).map_err(|_| SensorError::GainIndex(gain_index))?;
                return Ok(ExposureRegisters {
                    data: [
                        Register(0x3366, 0xff00 | (gain << 4) | gain),
                        Register(0x3362, u32::from(dc_gain)),
                        Register(0x3012, time & 0xffff),
                        Register::default(),
                        Register::default(),
                        Register::default(),
                        Register::default(),
                        Register::default(),
                        Register::default(),
                    ],
                    len: 3,
                });
            }
            Self::Ox03c10 => {
                let gain = data::OX03C10_GAIN_REGISTERS[gain_index_usize];
                let spd_time = time.clamp(2050 / 3, 2050);
                let vs_time = (time / 40).clamp(1, 34);
                [
                    Register(0x3501, time >> 8),
                    Register(0x3502, time & 0xff),
                    Register(0x3581, time >> 8),
                    Register(0x3582, time & 0xff),
                    Register(0x3541, spd_time >> 8),
                    Register(0x3542, spd_time & 0xff),
                    Register(0x35c2, vs_time & 0xff),
                    Register(0x3508, gain >> 8),
                    Register(0x3509, gain & 0xff),
                ]
            }
            Self::Os04c10 => {
                let gain = data::OS04C10_GAIN_REGISTERS[gain_index_usize];
                [
                    Register(0x3208, 0),
                    Register(0x3501, time >> 8),
                    Register(0x3502, time & 0xff),
                    Register(0x3508, gain >> 8),
                    Register(0x3509, gain & 0xff),
                    Register(0x350c, gain >> 8),
                    Register(0x350d, gain & 0xff),
                    Register(0x3208, 0x10),
                    Register(0x3208, 0xa0),
                ]
            }
        };
        Ok(ExposureRegisters { data, len: 9 })
    }
}
