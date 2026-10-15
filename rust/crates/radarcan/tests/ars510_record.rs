use openpilot_radarcan::decoder::toyota::ars510::record::*;
#[test]
fn pinned_record_crc_idle_slots_partial_slices_and_allocation_count_match() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-record-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 512);
    for (index, row) in rows.iter().enumerate() {
        let record: Vec<u8> = serde_json::from_value(row["record"].clone()).unwrap();
        assert_eq!(
            serde_json::json!(id80_crc(&record)),
            row["crc"],
            "crc/{index}"
        );
        assert_eq!(
            serde_json::json!(id80_crc_ok(&record)),
            row["valid"],
            "valid/{index}"
        );
        assert_eq!(
            serde_json::json!(occupied_slots(&record).collect::<Vec<_>>()),
            row["occupied"],
            "occupied/{index}"
        );
        assert_eq!(
            serde_json::json!(live_object_count(&record)),
            row["live"],
            "live/{index}"
        );
        assert_eq!(
            serde_json::json!((0..23)
                .map(|slot| slot_bytes(&record, slot))
                .collect::<Vec<_>>()),
            row["slots"],
            "slices/{index}"
        );
    }
}
