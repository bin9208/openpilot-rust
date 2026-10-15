use openpilot_radarcan::decoder::hyundai::bosch::assignment::solve;
#[test]
fn pinned_numpy_assignment_matches_duals_ties_transposition_and_errors() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-assignment-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2048);
    for (index, row) in rows.iter().enumerate() {
        let cost: Vec<Vec<f64>> = row["cost"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|value| {
                        value
                            .as_f64()
                            .unwrap_or_else(|| match value.as_str().unwrap() {
                                "nan" => f64::NAN,
                                "inf" => f64::INFINITY,
                                "-inf" => f64::NEG_INFINITY,
                                _ => panic!("cost"),
                            })
                    })
                    .collect()
            })
            .collect();
        let result = match solve(&cost, usize::try_from(row["m"].as_u64().unwrap()).unwrap()) {
            Ok(assignment) => serde_json::to_value(assignment).unwrap(),
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, row["result"], "matrix/{index}");
    }
}
