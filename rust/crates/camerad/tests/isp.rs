use openpilot_camerad::{isp::ife, sensor::SensorKind};

#[test]
fn bps_keeps_the_ar0231_blob_alias_and_zero_first_linearization_segment() {
    use openpilot_camerad::isp::bps;
    let ar = bps::lookup_tables(SensorKind::Ar0231);
    let ox = bps::lookup_tables(SensorKind::Ox03c10);
    assert_eq!(ar.config, ox.config);
    assert_eq!(
        &ar.config[..12],
        &[3, 0, 0, 0, 0x88, 7, 0, 0, 0xb8, 4, 0, 0]
    );
    let lookup = bps::linearization(SensorKind::Os04c10.config());
    assert_eq!(&lookup[..4], &[0; 4]);
}

#[test]
fn initial_ife_config_keeps_all_six_kernel_address_patches() {
    let sensor = SensorKind::Os04c10.config();
    let program = ife::initial(sensor, true, 1928, 1208).unwrap();
    assert_eq!(program.patches.len(), 6);
    for offset in program.patches {
        assert_eq!(
            &program.bytes[offset as usize..offset as usize + 4],
            &[0; 4]
        );
    }
}

#[test]
fn per_frame_ife_config_contains_source_black_level_register() {
    let sensor = SensorKind::Ar0231.config();
    let program = ife::update(sensor, false).unwrap();
    assert!(program.patches.is_empty());
    let tail = &program.bytes[program.bytes.len() - 20..];
    assert_eq!(&tail[4..8], &0x6b0_u32.to_le_bytes());
    assert_eq!(
        &tail[8..12],
        &((1_u32 << 26) | (sensor.black_level << (14 - sensor.bits_per_pixel))).to_le_bytes()
    );
}
