use openpilot_card::brands::hyundai::{
    bus::CanBus,
    detection::{bosch_mrrevo14f_bus, detect, DetectionInput},
    flags,
};
#[test]
fn source_exact_bosch_full_message_set_bus_preference_and_capability_flags() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-capability-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(), 1920);
    for (index, row) in rows.iter().enumerate() {
        let candidate = row["candidate"].as_str().unwrap();
        let flag = row["flags"].as_u64().unwrap() as u32;
        let fp: Vec<(u8, Vec<(u32, usize)>)> = serde_json::from_value(row["fp"].clone()).unwrap();
        let buses: Vec<u8> = serde_json::from_value(row["buses"].clone()).unwrap();
        let expected: Option<u8> = serde_json::from_value(row["result"].clone()).unwrap();
        assert_eq!(
            bosch_mrrevo14f_bus(candidate, flag, &fp, &buses),
            expected,
            "bus/{index}"
        );
        if buses == [2, 1] && fp.iter().all(|(bus, _)| *bus < 4) {
            let detection = detect(DetectionInput {
                candidate,
                fingerprints: &fp,
                flags: flag,
                has_radar_dbc: true,
                camera_scc: 0,
                hda2: 0,
                radar_tracks: 0,
                alpha_long: false,
            });
            let actual =
                detection.ext_flags & (flags::ext::BOSCH_RADAR | flags::ext::BOSCH_RADAR_BUS1);
            let expected = expected.map_or(0, |bus| {
                flags::ext::BOSCH_RADAR
                    | if bus == CanBus::with_offset(0, false, 0).acan {
                        flags::ext::BOSCH_RADAR_BUS1
                    } else {
                        0
                    }
            });
            assert_eq!(actual, expected, "flags/{index}");
        }
    }
}
