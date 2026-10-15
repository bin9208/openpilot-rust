use openpilot_card::brands::hyundai::stopping::{CanfdStopping, StopInput, StopPhase};
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

fn clean(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) if matches!(s.as_str(), "NaN" | "Infinity" | "-Infinity") => {
            serde_json::json!(0.)
        }
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(clean).collect()),
        serde_json::Value::Object(o) => {
            serde_json::Value::Object(o.iter().map(|(k, v)| (k.clone(), clean(v))).collect())
        }
        v => v.clone(),
    }
}
fn number(value: &serde_json::Value) -> f64 {
    match value.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => value.as_f64().unwrap(),
    }
}
#[test]
fn all_144_pinned_stopping_tests_match_native_22907_complete_state_transitions() {
    let data =
        zstd::stream::decode_all(include_bytes!("fixtures/stopping-final.json.zst").as_slice())
            .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let mut controller: CanfdStopping = serde_json::from_value(clean(&row["before"])).unwrap();
        if row["before"].get("last_scc_jerk_upper").is_some() {
            controller.last_scc_jerk_upper = number(&row["before"]["last_scc_jerk_upper"]);
        }
        let input = &row["input"];
        let result = match row["operation"].as_str().unwrap() {
            "update" => {
                let mut args: StopInput = serde_json::from_value(clean(input)).unwrap();
                args.speed = number(&input["speed"]);
                args.accel = number(&input["accel"]);
                args.previous_value = number(&input["previous_value"]);
                args.jerk_u = number(&input["jerk_u"]);
                args.jerk_l = number(&input["jerk_l"]);
                serde_json::json!(controller.update(args))
            }
            "reset" => {
                controller.reset();
                serde_json::Value::Null
            }
            "enter" => {
                let phase: StopPhase = serde_json::from_value(input["phase"].clone()).unwrap();
                controller.enter(phase, input["reason"].as_str().unwrap().to_owned());
                serde_json::Value::Null
            }
            "limit_scc_jerk_upper" => serde_json::json!(controller.limit_scc_jerk_upper(
                u8::try_from(input["stop_req"].as_u64().unwrap()).unwrap(),
                number(&input["jerk_upper"])
            )),
            other => panic!("unknown operation {other}"),
        };
        compare(&result, &row["result"], &format!("result/{i}"));
        compare(
            &serde_json::to_value(controller).unwrap(),
            &row["state"],
            &format!("state/{i}"),
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 22907);
}
