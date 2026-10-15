use openpilot_card::brands::hyundai::{cluster_fields, wire::Values};
#[test]
fn all_11520_pinned_hud_dm_precedence_vectors_match() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/hud-dm-final.json.zst").as_slice())
            .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 11520);
    for (index, row) in rows.iter().enumerate() {
        let mut data: Values = serde_json::from_value(row["before"].clone()).unwrap();
        let stock: Values = serde_json::from_value(row["stock"].clone()).unwrap();
        cluster_fields::driver_monitoring_alert(
            &mut data,
            u8::try_from(row["level"].as_u64().unwrap()).unwrap(),
            &stock,
        )
        .unwrap();
        assert_eq!(
            data,
            serde_json::from_value::<Values>(row["after"].clone()).unwrap(),
            "source vector {index}"
        );
    }
}
