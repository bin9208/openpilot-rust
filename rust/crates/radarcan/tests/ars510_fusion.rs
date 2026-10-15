use openpilot_radarcan::decoder::toyota::ars510::{fusion::Fusion, objects::NativeObject};
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
fn value(number: f64) -> serde_json::Value {
    if number.is_nan() {
        serde_json::json!("NaN")
    } else if number == f64::INFINITY {
        serde_json::json!("Infinity")
    } else if number == f64::NEG_INFINITY {
        serde_json::json!("-Infinity")
    } else {
        serde_json::json!(number)
    }
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
fn pinned_white_colored_speed_and_range_filters_match_complete_states_errors_and_bits() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/ars510-fusion-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2736);
    for (index, row) in rows.iter().enumerate() {
        let mut model: Fusion = serde_json::from_value(row["before"].clone()).unwrap();
        let input = &row["input"];
        let result = match row["operation"].as_str().unwrap() {
            "_fused_speed" => {
                let object: NativeObject = serde_json::from_value(input["obj"].clone()).unwrap();
                let extra: Vec<(f64, f64)> =
                    serde_json::from_value(input["extra"].clone()).unwrap();
                model
                    .speed(
                        input["tid"].as_u64().unwrap(),
                        number(&input["time_s"]),
                        &object,
                        &extra,
                        number(&input["v_ego"]),
                    )
                    .map(|(speed, std)| serde_json::json!([value(speed), value(std)]))
            }
            "_fused_range" => Ok(value(model.range(
                input["tid"].as_u64().unwrap(),
                number(&input["time_s"]),
                number(&input["d_meas"]),
                number(&input["vrel"]),
            ))),
            other => panic!("operation {other}"),
        };
        if let Some(expected) = row.get("error") {
            let error = result.unwrap_err();
            let kind = match error {
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
            compare(&result.unwrap(), &row["result"], &format!("result/{index}"));
        }
        compare(
            &serde_json::to_value(model).unwrap(),
            &row["after"],
            &format!("state/{index}"),
        );
    }
}
