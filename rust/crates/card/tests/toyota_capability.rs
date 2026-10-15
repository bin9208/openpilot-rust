#[derive(serde::Deserialize)]
struct Row {
    brand: String,
    candidate: String,
    flags: u32,
    expected: bool,
}
#[test]
fn pinned_receiver_capability_is_shared_by_card_and_radar_factory() {
    let rows: Vec<Row> = serde_json::from_slice(
        &zstd::stream::decode_all(
            &include_bytes!("fixtures/toyota-ars510-capability-final.json.zst")[..],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1824);
    for (index, row) in rows.into_iter().enumerate() {
        assert_eq!(
            openpilot_card::brands::toyota::supports_ars510(&row.brand, &row.candidate, row.flags),
            row.expected,
            "source capability {index}"
        );
    }
}
