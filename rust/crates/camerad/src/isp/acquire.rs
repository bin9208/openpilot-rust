use crate::{packet::u32_at, sensor::SensorConfig};

pub fn ife_port(sensor: &SensorConfig, phy: u32, raw: bool, width: u32, height: u32) -> [u8; 132] {
    let mut data = [0; 132];
    let line_start = if raw { 0 } else { sensor.frame_offset };
    let input_height = sensor.frame_height.wrapping_add(if raw {
        sensor.extra_height
    } else {
        sensor.frame_offset
    });
    let fields = [
        (0, phy),
        (8, 4),
        (12, 0x3210),
        (20, sensor.frame_data_type),
        (24, sensor.mipi_format),
        (28, sensor.bayer_pattern),
        (40, sensor.frame_width.wrapping_sub(1)),
        (44, sensor.frame_width),
        (52, sensor.frame_width.wrapping_sub(1)),
        (56, sensor.frame_width),
        (60, line_start),
        (64, input_height.wrapping_sub(1)),
        (68, input_height),
        (96, 1),
        (100, if raw { 0x3006 } else { 0x3000 }),
        (104, if raw { sensor.mipi_format } else { 32 }),
        (108, width),
        (112, height.wrapping_add(sensor.extra_height)),
    ];
    for (offset, value) in fields {
        u32_at(&mut data, offset, value);
    }
    data
}

pub fn bps_resource(
    sensor: &SensorConfig,
    config_handle: i32,
    config_size: u32,
    width: u32,
    height: u32,
) -> [u8; 60] {
    let mut data = [0; 60];
    for (offset, value) in [
        (4, 1),
        (8, config_size),
        (12, config_handle as u32),
        (24, 9),
        (28, sensor.frame_width),
        (32, sensor.frame_height),
        (36, 20),
        (40, 1),
        (44, 3),
        (48, width),
        (52, height),
        (56, 20),
    ] {
        u32_at(&mut data, offset, value);
    }
    data
}
