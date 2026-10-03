use super::{bps_data, Program};
use crate::{
    cdm::PackingError,
    sensor::{SensorConfig, SensorKind},
};

pub struct Tables {
    pub config: &'static [u8; 768],
    pub settings: &'static [u8; 684],
    pub striping: &'static [u8; 3160],
}

pub fn lookup_tables(sensor: SensorKind) -> Tables {
    let index = match sensor {
        SensorKind::Ar0231 | SensorKind::Ox03c10 => 2,
        SensorKind::Os04c10 => 3,
    };
    Tables {
        config: &bps_data::BPS_CFG[index],
        settings: &bps_data::BPS_SETTINGS[index],
        striping: &bps_data::BPS_STRIPING_OUTPUT[index],
    }
}

pub fn linearization(sensor: &SensorConfig) -> [u32; 36] {
    let mut output = [0; 36];
    let black = sensor.black_level << (14 - sensor.bits_per_pixel);
    for (index, value) in output.iter_mut().enumerate().skip(4) {
        let entry = sensor.linearization_lut[index - 4];
        let base = (entry & 0x3fff).saturating_sub(black);
        let slope = (((entry >> 14) & 0x3fff) << 1).min(0x3fff);
        *value = base | (slope << 14);
    }
    output
}

pub fn program(sensor: &SensorConfig) -> Result<Program, PackingError> {
    let mut knee = [0; 8];
    knee[0] = sensor.black_level << (14 - sensor.bits_per_pixel);
    for index in 0..7 {
        let points = sensor.linearization_pts[index / 2];
        knee[index + 1] = if index % 2 == 0 {
            points >> 16
        } else {
            points & 0xffff
        };
    }
    let linear: [u32; 4] =
        std::array::from_fn(|index| (knee[2 * index + 1] << 16) | knee[2 * index]);
    let color: [u32; 6] = std::array::from_fn(|index| {
        let row = index / 2;
        if index % 2 == 0 {
            sensor.color_correct_matrix[row] | (sensor.color_correct_matrix[row + 3] << 16)
        } else {
            sensor.color_correct_matrix[row + 6]
        }
    });
    let mut program = Program::default();
    program.cont(0x2868, &[0x04000400, 0x400, 0, 0])?;
    program.cont(0x2878, &[0x80, 0x00800066])?;
    for register in [0x1868, 0x1878, 0x1888, 0x1898] {
        program.cont(register, &linear)?;
    }
    program.dmi_opcode(sensor.linearization_lut.len() as u32 * 4, 0x1808, 1, 1)?;
    program.cont(0x2e68, &color)?;
    for channel in 1..=3 {
        program.dmi_opcode(sensor.gamma_lut_rgb.len() as u32 * 4, 0x3208, channel, 1)?;
    }
    program.yuv(false)?;
    Ok(program)
}
