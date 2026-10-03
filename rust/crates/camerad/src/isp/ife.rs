use super::Program;
use crate::{cdm::PackingError, sensor::SensorConfig};

pub fn update(sensor: &SensorConfig, vignetting: bool) -> Result<Program, PackingError> {
    let mut program = Program::default();
    program.random(&[
        0x2c,
        u32::MAX,
        0x30,
        u32::MAX,
        0x34,
        u32::MAX,
        0x38,
        u32::MAX,
        0x3c,
        u32::MAX,
    ])?;
    program.cont(
        0x560,
        &[
            1, 0x04440444, 0x04450445, 0x04440444, 0x04450445, 0xca, 0x9c,
        ],
    )?;
    program.cont(0x6fc, &[0x00800080, 0x80, 0, 0])?;
    program.cont(0x40, &[0xc06 | (u32::from(vignetting) << 8)])?;
    program.cont(0x44, &[0])?;
    program.cont(0x48, &[10])?;
    program.cont(0x4c, &[0x19])?;
    program.cont(0xf00, &[0])?;
    program.cont(0xe0c, &[0xe00])?;
    program.cont(0xe2c, &[0xe00])?;
    program.cont(
        0x6b0,
        &[
            (1 << 26) | (sensor.black_level << (14 - sensor.bits_per_pixel)),
            0,
            0,
        ],
    )?;
    Ok(program)
}

pub fn initial(
    sensor: &SensorConfig,
    vignetting: bool,
    width: u32,
    height: u32,
) -> Result<Program, PackingError> {
    let mut program = update(sensor, vignetting)?;
    program.cont(0x478, &[4, 0x004000c0])?;
    program.cont(0x488, &[0, 0, 0xf0f])?;
    program.cont(0x49c, &[1])?;
    program.cont(0xce4, &[0, 0])?;
    program.cont(0x4dc, &[0])?;
    for register in [0x4e0, 0x4f0, 0x500, 0x510] {
        program.cont(register, sensor.linearization_pts)?;
    }
    program.dmi(sensor.linearization_lut.len() as u32 * 4, 0xc24, 9)?;
    program.cont(0x6bc, &[0x0b3c0000, 0x00670067, 0xd3b1300c, 0x13b1300c])?;
    program.cont(0x6d8, &[0xec4e4000, 0x0100c003])?;
    for selector in [14, 15] {
        program.dmi(sensor.vignetting_lut.len() as u32 * 4, 0xc24, selector)?;
    }
    program.cont(0x6f8, &[0x100])?;
    program.cont(0x71c, &[0x8000, 0x08000066])?;
    program.cont(0x760, sensor.color_correct_matrix)?;
    program.cont(0x798, &[0])?;
    for selector in [26, 28, 30] {
        program.dmi(sensor.gamma_lut_rgb.len() as u32 * 4, 0xc24, selector)?;
    }
    for (register, width, height, scale) in [
        (0xa3c, width, height, 0x30036666),
        (0xa68, width / 2, height / 2, 0x3006cccc),
    ] {
        program.cont(
            register,
            &[
                3,
                (width.wrapping_sub(1) << 16) | sensor.frame_width.wrapping_sub(1),
                scale,
                0,
                0,
                sensor.frame_width.wrapping_sub(1),
                (height.wrapping_sub(1) << 16) | sensor.frame_height.wrapping_sub(1),
                scale,
                0,
                0,
                sensor.frame_height.wrapping_sub(1),
            ],
        )?;
    }
    program.cont(0xe10, &[height.wrapping_sub(1), width.wrapping_sub(1)])?;
    program.cont(
        0xe30,
        &[(height / 2).wrapping_sub(1), width.wrapping_sub(1)],
    )?;
    program.cont(0xe18, &[0x0ff00000, 0x16])?;
    program.cont(0xe38, &[0x0ff00000, 0x17])?;
    program.yuv(true)?;
    Ok(program)
}
