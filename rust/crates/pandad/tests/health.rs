use openpilot_pandad::health::{CanHealth, Health};

#[test]
fn packed_health_fields_preserve_unaligned_offsets_and_wire_bits() {
    let mut packet = [0; 58];
    for (index, byte) in packet.iter_mut().enumerate() {
        *byte = u8::try_from(index).unwrap();
    }
    let health = Health::from_packet(&packet);
    assert_eq!(health.uptime, 0x03020100);
    assert_eq!(health.voltage, 0x07060504);
    assert_eq!(health.faults, 0x1f1e1d1c);
    assert_eq!(health.safety_model, 0x24);
    assert_eq!(health.safety_param, 0x2625);
    assert_eq!(health.alternative_experience, 0x2b2a);
    assert_eq!(health.interrupt_load.to_bits(), 0x2f2e2d2c);
    assert_eq!(health.spi_checksum_errors, 0x3332);
    assert_eq!(health.sbu1_mv, 0x3635);
    assert_eq!(health.sbu2_mv, 0x3837);
    assert_eq!(health.som_reset_triggered, 0x39);
}

#[test]
fn packed_can_health_preserves_unaligned_counters_and_trailing_reset_count() {
    let mut packet = [0; 64];
    for (index, byte) in packet.iter_mut().enumerate() {
        *byte = u8::try_from(index).unwrap();
    }
    let health = CanHealth::from_packet(&packet);
    assert_eq!(health.bus_off_count, 0x04030201);
    assert_eq!(health.last_data_error, 9);
    assert_eq!(health.total_errors, 0x100f0e0d);
    assert_eq!(health.total_tx_checksum_errors, 0x28272625);
    assert_eq!(health.can_speed, 0x2a29);
    assert_eq!(health.can_data_speed, 0x2c2b);
    assert_eq!(health.canfd_non_iso, 47);
    assert_eq!(health.irq0_rate, 0x33323130);
    assert_eq!(health.irq1_rate, 0x37363534);
    assert_eq!(health.irq2_rate, 0x3b3a3938);
    assert_eq!(health.core_reset_count, 0x3f3e3d3c);
}
