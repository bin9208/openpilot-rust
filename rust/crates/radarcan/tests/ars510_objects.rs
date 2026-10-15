use openpilot_radarcan::decoder::toyota::ars510::{
    fields::NAMED_FIELDS,
    objects::{decode_native_slot, encode_slot, field_code, field_value, relative_velocity},
};
#[test]
fn pinned_ars510_object_catalog_all_fields_and_slot_encodings_match() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-objects-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    assert_eq!(
        serde_json::to_value(NAMED_FIELDS).unwrap(),
        fixture["catalog"]
    );
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2048);
    for (index, row) in rows.iter().enumerate() {
        let payload: Vec<u8> = serde_json::from_value(row["payload"].clone()).unwrap();
        let result = match decode_native_slot(row["slot"].as_i64().unwrap(), &payload) {
            Ok(object) => {
                serde_json::json!({"relative":relative_velocity(&object,row["speed"].as_f64().unwrap()),"object":object})
            }
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, row["result"], "object/{index}");
        let fields: Vec<_> = NAMED_FIELDS
            .iter()
            .map(|field| {
                serde_json::json!([field_code(&payload, *field), field_value(&payload, *field)])
            })
            .collect();
        assert_eq!(serde_json::json!(fields), row["fields"], "fields/{index}");
    }
    for (index, row) in fixture["encodings"].as_array().unwrap().iter().enumerate() {
        let codes: Vec<(String, i64)> = serde_json::from_value(row["codes"].clone()).unwrap();
        let codes: Vec<_> = codes
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        let result = match encode_slot(&codes) {
            Ok(payload) => serde_json::json!({"payload":payload.as_slice()}),
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, row["result"], "encoding/{index}");
    }
}
