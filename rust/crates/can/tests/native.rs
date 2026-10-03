use openpilot_can::{dbc::Dbc, packer::Packer, parser::Parser, Frame, Packet};
use std::sync::Arc;

fn dbc() -> Arc<Dbc> {
    Arc::new(
        Dbc::parse(
            "test",
            include_str!("../../../../opendbc_repo/opendbc/can/tests/test.dbc"),
        )
        .unwrap(),
    )
}

#[test]
fn warnings_keep_message_registration_order_when_multiple_messages_expire() {
    let database = Arc::new(Dbc::parse("order", "BO_ 500 HIGH: 1 XXX\n SG_ VALUE : 0|8@1+ (1,0) [0|255] \"\" XXX\nBO_ 100 LOW: 1 XXX\n SG_ VALUE : 0|8@1+ (1,0) [0|255] \"\" XXX\n").unwrap());
    let mut parser = Parser::new(database, 0, 1);
    parser.add("HIGH", Some(100.), false, 1).unwrap();
    parser.add("LOW", Some(100.), false, 1).unwrap();
    parser
        .update(&[Packet {
            mono_time: 3_000_000_000,
            frames: vec![],
        }])
        .unwrap();
    assert!(!parser.can_valid());
    assert_eq!(
        parser
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.address)
            .collect::<Vec<_>>(),
        [500, 100]
    );
}

#[test]
fn automatic_counter_wraps_and_explicit_counter_restarts_at_the_override() {
    let mut packer = Packer::new(dbc());
    for counter in 0..1000 {
        let data = packer.pack("CAN_FD_MESSAGE", &[], None).unwrap();
        assert_eq!(data[0], u8::try_from(counter % 256).unwrap());
    }
    assert_eq!(
        packer
            .pack("CAN_FD_MESSAGE", &[("COUNTER", 200.)], None)
            .unwrap()[0],
        200
    );
    assert_eq!(packer.pack("CAN_FD_MESSAGE", &[], None).unwrap()[0], 200);
    assert_eq!(packer.pack("CAN_FD_MESSAGE", &[], None).unwrap()[0], 201);
}

#[test]
fn parser_preserves_packet_history_and_wrong_bus_does_not_refresh_bus_timeout() {
    let database = dbc();
    let mut parser = Parser::new(Arc::clone(&database), 0, 1_000_000_000);
    parser
        .add("CAN_FD_MESSAGE", Some(100.), false, 1_000_000_000)
        .unwrap();
    let mut packer = Packer::new(database);
    let first = packer
        .pack("CAN_FD_MESSAGE", &[("SIGNED", -10.)], None)
        .unwrap();
    let second = packer
        .pack("CAN_FD_MESSAGE", &[("SIGNED", 12.)], None)
        .unwrap();
    let updated = parser
        .update(&[
            Packet {
                mono_time: 1_010_000_000,
                frames: vec![Frame {
                    address: 245,
                    data: first.clone(),
                    bus: 0,
                }],
            },
            Packet {
                mono_time: 1_020_000_000,
                frames: vec![Frame {
                    address: 245,
                    data: second.clone(),
                    bus: 0,
                }],
            },
        ])
        .unwrap();
    assert!(updated.contains(&245));
    assert_eq!(parser.signal("CAN_FD_MESSAGE", "SIGNED").unwrap(), 12.);
    assert_eq!(parser.states[&245].all_values[1], [-10., 12.]);
    assert_eq!(parser.raw[&245], second);
    assert!(parser.can_valid());
    parser
        .update(&[Packet {
            mono_time: 1_200_000_001,
            frames: vec![Frame {
                address: 245,
                data: first,
                bus: 1,
            }],
        }])
        .unwrap();
    assert!(parser.bus_timeout());
}

#[test]
fn duplicate_message_check_is_an_error() {
    let mut parser = Parser::new(dbc(), 0, 1);
    parser.add("CAN_FD_MESSAGE", Some(100.), false, 1).unwrap();
    assert!(parser.add("CAN_FD_MESSAGE", Some(100.), false, 1).is_err());
}

#[test]
fn registered_frequency_preserves_source_threshold_rounding() {
    let mut parser = Parser::new(dbc(), 0, 1);
    parser.add("CAN_FD_MESSAGE", Some(33.), false, 1).unwrap();
    assert_eq!(
        parser.states[&245].timeout_threshold.to_bits(),
        0x41b2_0fe0_1f07_c1f1,
    );
}

#[test]
fn inherited_mlb_checksum_signature_failure_is_preserved() {
    let source = "BO_ 265 ACC_01: 8 XXX\n SG_ CHECKSUM : 0|8@1+ (1,0) [0|255] \"\" XXX\n";
    let dbc = Arc::new(Dbc::parse("vw_mlb", source).unwrap());
    let mut packer = Packer::new(dbc);
    assert!(packer.pack("ACC_01", &[], None).is_err());
}

#[test]
fn lazy_lookup_registers_zero_values_then_receives_only_future_packets() {
    let database = dbc();
    let mut packer = Packer::new(Arc::clone(&database));
    let data = packer
        .pack("CAN_FD_MESSAGE", &[("SIGNED", -7.)], None)
        .unwrap();
    let packet = Packet {
        mono_time: 1_000_000_000,
        frames: vec![Frame {
            address: 245,
            data,
            bus: 0,
        }],
    };
    let mut parser = Parser::new(database, 0, 1);
    assert!(parser
        .update(std::slice::from_ref(&packet))
        .unwrap()
        .is_empty());
    assert_eq!(
        parser
            .signal_lazy("CAN_FD_MESSAGE", "SIGNED", 1_000_000_000)
            .unwrap(),
        0.
    );
    assert!(parser.update(&[packet]).unwrap().contains(&245));
    assert_eq!(parser.signal("CAN_FD_MESSAGE", "SIGNED").unwrap(), -7.);
}
