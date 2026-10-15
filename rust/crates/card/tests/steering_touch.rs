use openpilot_card::brands::hyundai::steering_touch;
use steering_touch::SteeringTouch;
#[test]
fn source_profile_checksums_counter_rollover_staleness_and_bad_frames_match() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/steering-touch-final.json")).unwrap();
    let mut monitor = SteeringTouch::default();
    let mut valid = 0;
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let data = row["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let result = monitor.update(
            row["profile"].as_bool().unwrap(),
            row["timestamp"].as_u64().unwrap(),
            row["now"].as_u64().unwrap(),
            row["bus_timeout"].as_bool().unwrap(),
            &data,
        );
        valid += usize::from(result.as_ref().is_some_and(|s| s.valid));
        let output = result.map_or(serde_json::json!({}), |r| serde_json::to_value(r).unwrap());
        assert_eq!(output, row["result"], "output {i}");
        assert_eq!(
            serde_json::to_value(&monitor).unwrap(),
            row["state"],
            "state {i}"
        );
    }
    assert_eq!(valid, 469);
}
#[test]
fn independent_recorded_receive_checksum_vectors() {
    assert_eq!(steering_touch::ADDRESS, 0x2af);
    assert_eq!(steering_touch::MESSAGE, "STEER_TOUCH_2AF");
    for bytes in [
        [0x7e, 0x60, 0, 1, 0x0c, 0x0e, 1, 0],
        [0x44, 0x40, 1, 1, 0x1e, 0x10, 1, 0],
        [0x9e, 0xa0, 2, 1, 0x33, 0x16, 1, 0],
        [0x0b, 0x30, 3, 1, 0x37, 0x1b, 1, 0],
    ] {
        assert_eq!(steering_touch::touch_checksum(&bytes[1..]), bytes[0]);
    }
}

#[test]
fn native_parser_late_discovery_is_optional_and_uses_only_original_ecan() {
    use openpilot_can::{dbc::Dbc, parser::Parser, Frame, Packet};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../opendbc_repo/opendbc/dbc/hyundai_canfd_generated.dbc");
    let dbc = std::sync::Arc::new(Dbc::load(&path).unwrap());
    let mut parser = Parser::new(dbc, 4, 0);
    parser.controls_ready = true;
    let mut monitor = SteeringTouch::default();
    let payload = |counter: u8| {
        let mut data = vec![0, counter << 4, 1, 1, 19, 14, 1, 0];
        data[0] = steering_touch::touch_checksum(&data[1..]);
        data
    };
    for bus in [6, 132, 134] {
        parser
            .update(&[Packet {
                mono_time: 1_000_000_000,
                frames: vec![Frame {
                    address: 0x2af,
                    bus,
                    data: payload(0),
                }],
            }])
            .unwrap();
        assert!(
            !monitor
                .update_parser(&mut parser)
                .unwrap()
                .unwrap()
                .available
        );
        assert!(parser.states.is_empty());
    }
    for i in 0..3 {
        parser
            .update(&[Packet {
                mono_time: 1_000_000_000 + i * 10_000_000,
                frames: vec![Frame {
                    address: 0x2af,
                    bus: 4,
                    data: payload(i as u8),
                }],
            }])
            .unwrap();
        let output = monitor.update_parser(&mut parser).unwrap().unwrap();
        assert_eq!(output.valid, i == 2);
    }
    assert!(parser.states[&0x2af].ignore_alive);
    assert!(parser.can_valid());
    for (index, data) in [
        vec![],
        payload(3)[..7].to_vec(),
        [vec![0], payload(3)[1..].to_vec()].concat(),
    ]
    .into_iter()
    .enumerate()
    {
        parser
            .update(&[Packet {
                mono_time: 1_030_000_000 + index as u64 * 10_000_000,
                frames: vec![Frame {
                    address: 0x2af,
                    bus: 4,
                    data,
                }],
            }])
            .unwrap();
        assert!(
            !monitor.update_parser(&mut parser).unwrap().unwrap().valid,
            "bad frame {index}"
        );
    }
}
