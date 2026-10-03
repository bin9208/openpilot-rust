use openpilot_camerad::{
    packet::{CommandBuffer, Packet},
    sensor::SensorKind,
    sensor_packets,
};

#[test]
fn packet_uses_payload_offset_not_sizeof_as_descriptor_start() {
    let mut packet = Packet::new(0x0100_0003, 0, 2, 0, 0).unwrap();
    packet
        .command(
            0,
            CommandBuffer {
                handle: 0x1234,
                size: 24,
                length: 24,
                kind: 10,
                ..CommandBuffer::default()
            },
        )
        .unwrap();
    assert_eq!(packet.bytes().len(), 112);
    assert_eq!(&packet.bytes()[56..60], &0x1234_u32.to_le_bytes());
    assert_eq!(&packet.bytes()[104..112], &[0; 8]);
}

#[test]
fn sensor_probe_preserves_reset_power_sequence_and_waits() {
    let probe = sensor_packets::probe(SensorKind::Os04c10, 2, 101, 102).unwrap();
    assert_eq!(probe.power.len(), 196);
    assert_eq!(&probe.power[52..56], &[1, 0, 3, 9]);
    assert_eq!(&probe.power[92..96], &[34, 0, 3, 9]);
    assert_eq!(&probe.info[..4], &[0x6c, 0, 1, 4]);
    assert_eq!(&probe.info[20..22], &[2, 0]);
}

#[test]
fn sensor_poke_sign_extends_original_signed_request_argument() {
    let packet = sensor_packets::poke(i32::MIN).unwrap();
    assert_eq!(
        &packet.bytes()[8..16],
        &0xffff_ffff_8000_0000_u64.to_le_bytes()
    );
}
