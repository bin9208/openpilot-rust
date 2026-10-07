use openpilot_camerad::{isp::acquire, sensor::SensorKind};

#[test]
fn raw_isp_acquisition_includes_sensor_extra_lines() {
    let sensor = SensorKind::Ar0231.config();
    let raw = acquire::ife_port(sensor, 0x4002, true, 1928, 1208);
    assert_eq!(raw.len(), 132);
    assert_eq!(&raw[60..64], &[0; 4]);
    assert_eq!(
        &raw[64..68],
        &(sensor.frame_height + sensor.extra_height - 1).to_le_bytes()
    );
    assert_eq!(&raw[100..104], &0x3006_u32.to_le_bytes());
}

#[test]
fn bps_acquisition_has_packed_output_resource_after_count() {
    let sensor = SensorKind::Os04c10.config();
    let resource = acquire::bps_resource(sensor, -7, 768, 1344, 760);
    assert_eq!(resource.len(), 60);
    assert_eq!(&resource[12..16], &(-7_i32).to_le_bytes());
    assert_eq!(&resource[40..48], &[1, 0, 0, 0, 3, 0, 0, 0]);
    assert_eq!(&resource[48..52], &1344_u32.to_le_bytes());
}
