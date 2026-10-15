fn number(value: &serde_json::Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| match value.as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("float"),
        })
}
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}: {a} != {b}"
        ),
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{index}"));
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, a) in a {
                compare(a, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn pinned_complete_native_frames_preserve_all_state_fields_points_errors_and_bits() {
    let decoder = zstd::stream::read::Decoder::new(
        include_bytes!("fixtures/ars510-native-frame-final.jsonl.zst").as_slice(),
    )
    .unwrap();
    let rows = serde_json::Deserializer::from_reader(std::io::BufReader::new(decoder)).into_iter::<serde_json::Value>();
    let mut count = 0;
    for (index, row) in rows.enumerate() {
        let row = row.unwrap();
        count += 1;
        let mut model: openpilot_radarcan::decoder::toyota::ars510::interface::NativeInterface =
            serde_json::from_value(row["before"].clone())
                .unwrap_or_else(|e| panic!("before/{index}: {e}"));
        compare(
            &serde_json::to_value(&model).unwrap(),
            &row["before"],
            &format!("before/{index}"),
        );
        let data: Vec<u8> = serde_json::from_value(row["data"].clone()).unwrap();
        let result = model.update_frame(
            number(&row["time"]),
            row["bus"].as_i64().unwrap(),
            row["address"].as_u64().unwrap() as u32,
            &data,
        );
        if let Some(expected) = row.get("error") {
            let error = result.err().expect("expected error");
            let kind = match error {
                openpilot_radarcan::Error::ArsValue(_) => "ValueError",
                openpilot_radarcan::Error::DivisionByZero => "ZeroDivisionError",
                openpilot_radarcan::Error::PowerOverflow => "OverflowError",
                _ => panic!("unexpected {error}"),
            };
            assert_eq!(
                serde_json::json!({"kind":kind,"message":error.to_string()}),
                *expected,
                "error/{index}"
            );
        } else {
            compare(
                &serde_json::to_value(result.unwrap()).unwrap(),
                &row["result"],
                &format!("result/{index}"),
            );
        }
        compare(
            &serde_json::to_value(model).unwrap(),
            &row["after"],
            &format!("state/{index}"),
        );
    }
    assert_eq!(count, 35881);
}
