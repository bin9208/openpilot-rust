#[path = "../src/brands/hyundai/navigation/section_average.rs"]
mod section_average;
use section_average::SectionAverage;
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert_eq!(a.to_bits(), b.to_bits(), "{path}: {a:?} != {b:?}");
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"));
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn complete_pinned_source_seven_regressions_match_native_section_state_and_allowance() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/section-average-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let mut states = std::collections::BTreeMap::new();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let state = states
            .entry(row["instance"].as_u64().unwrap())
            .or_insert_with(SectionAverage::default);
        let input = &row["input"];
        let number = |key: &str| input[key].as_f64().unwrap();
        let result = match row["operation"].as_str().unwrap() {
            "reset" => {
                state.reset();
                serde_json::Value::Null
            }
            "update" => {
                state.update(
                    input["section_active"].as_bool().unwrap(),
                    number("limit_kph"),
                    number("now"),
                    number("total_distance"),
                );
                serde_json::Value::Null
            }
            "unlock" => {
                state.unlock();
                serde_json::Value::Null
            }
            "lock" => {
                state.lock();
                serde_json::Value::Null
            }
            "bank_seconds" => {
                serde_json::json!(state.bank_seconds(number("now"), number("total_distance")))
            }
            "allowance_kph" => {
                serde_json::json!(state.allowance_kph(number("now"), number("total_distance")))
            }
            other => panic!("unknown operation {other}"),
        };
        compare(&result, &row["result"], &format!("result/{i}"));
        compare(
            &serde_json::to_value(state).unwrap(),
            &row["state"],
            &format!("state/{i}"),
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 28389);
}
