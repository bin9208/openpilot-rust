use openpilot_can::{dbc::Dbc, packer::Packer, parser::Parser, Frame, Packet};
use std::{path::PathBuf, sync::Arc};

#[test]
fn exact_psa_asset_supports_actual_lka_packer_and_parser_without_runtime_admission() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../opendbc_repo/opendbc/dbc/psa_aee2010_r3.dbc");
    let dbc = Arc::new(Dbc::load(&path).unwrap());
    let mut packer = Packer::new(Arc::clone(&dbc));
    let mut parser = Parser::new(Arc::clone(&dbc), 0, 2_000_000_000);
    parser
        .add("LANE_KEEP_ASSIST", Some(20.), false, 2_000_000_000)
        .unwrap();
    let address = dbc.message("LANE_KEEP_ASSIST").unwrap().address;
    for (index, angle) in [-12.5, 0., 12.5].into_iter().enumerate() {
        let data = packer
            .pack(
                "LANE_KEEP_ASSIST",
                &[
                    ("DRIVE", 1.),
                    ("STATUS", 4.),
                    ("LXA_ACTIVATION", 1.),
                    ("TORQUE_FACTOR", 100.),
                    ("SET_ANGLE", angle),
                ],
                None,
            )
            .unwrap();
        let now = 2_000_000_000 + index as u64 * 50_000_000;
        parser
            .update(&[Packet {
                mono_time: now,
                frames: vec![Frame {
                    address,
                    data,
                    bus: 0,
                }],
            }])
            .unwrap();
        assert_eq!(
            parser.signal("LANE_KEEP_ASSIST", "SET_ANGLE").unwrap(),
            angle
        );
        assert_eq!(parser.signal("LANE_KEEP_ASSIST", "STATUS").unwrap(), 4.);
        assert_eq!(
            parser.signal("LANE_KEEP_ASSIST", "TORQUE_FACTOR").unwrap(),
            100.
        );
    }
}
