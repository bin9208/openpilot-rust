use openpilot_card::brands::hyundai::navigation::{
    self,
    policy::{Config, Context, Policy},
    Event, EventType, Output,
};
use openpilot_cereal::car_capnp::car_state::button_event::Type;
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() <= 1e-10, "{path}: {a} != {b}");
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

fn events(nav: &navigation::Navigation) -> serde_json::Value {
    serde_json::Value::Array(nav.events.iter().map(|e|serde_json::json!({"type":match e.kind {EventType::Camera=>"camera",EventType::Bump=>"bump",EventType::SpeedZone=>"speedZone"},"speed":e.speed,"target":e.target,"cancelled":e.cancelled})).collect())
}
#[test]
fn pinned_whole_carstate_navigation_methods_match_native_state_outputs_and_consumption() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/navigation-policy-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let path =
        std::env::temp_dir().join(format!("native-navigation-policy-{}", std::process::id()));
    let params = openpilot_params::Params::open(&path, "p").unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let config: Config = serde_json::from_value(row["config"].clone()).unwrap();
        let context: Context = serde_json::from_value(row["context"].clone()).unwrap();
        let mut policy: Policy = serde_json::from_value(row["before"].clone()).unwrap();
        let mut output: Output = serde_json::from_value(row["output_before"].clone()).unwrap();
        let input = &row["input"];
        let mut nav =
            navigation::Navigation::new(row["nav"]["wrapped"].as_bool().unwrap(), &params).unwrap();
        nav.camera_policy = serde_json::from_value(row["before"]["cameras"].clone()).unwrap();
        nav.mode = i32::try_from(row["nav"]["mode"].as_i64().unwrap()).unwrap();
        nav.total_distance = row["nav"]["total_distance"].as_f64().unwrap();
        nav.events = row["nav"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(id, e)| Event {
                id: u64::try_from(id).unwrap(),
                kind: match e["type"].as_str().unwrap() {
                    "bump" => EventType::Bump,
                    "camera" => EventType::Camera,
                    _ => panic!("kind"),
                },
                speed: e["speed"].as_f64().unwrap(),
                target: e["target"].as_f64().unwrap(),
                subtype: 0,
                cancelled: e["cancelled"].as_bool().unwrap(),
            })
            .collect();
        let result = match row["op"].as_str().unwrap() {
            "gas" => serde_json::json!(policy.gas_tap(input["pressed"].as_bool().unwrap())),
            "decel" => serde_json::json!(policy.decel_long_press(
                u8::try_from(input["button"].as_u64().unwrap()).unwrap(),
                &config
            )),
            "camera" => serde_json::json!(policy
                .camera(
                    &mut nav,
                    &output,
                    input["camera"].as_bool().unwrap(),
                    (
                        input["warning_active"].as_bool().unwrap(),
                        input["warning_speed"].as_f64().unwrap()
                    ),
                    context,
                    &config
                )
                .unwrap()),
            "bump" => {
                policy.bump(&mut nav, &mut output, context, &config);
                serde_json::Value::Null
            }
            "section" => {
                policy.section(
                    &nav,
                    &mut output,
                    context,
                    input["long_press"].as_bool().unwrap(),
                    &config,
                );
                serde_json::Value::Null
            }
            "buttons" => {
                let buttons = input["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| {
                        (
                            e[0].as_bool().unwrap(),
                            Type::try_from(u16::try_from(e[1].as_u64().unwrap()).unwrap()).unwrap(),
                        )
                    })
                    .collect();
                serde_json::json!(policy
                    .filter_buttons(buttons, input["long_press"].as_bool().unwrap())
                    .into_iter()
                    .map(|(pressed, kind)| (pressed, u16::from(kind)))
                    .collect::<Vec<_>>())
            }
            other => panic!("unknown operation {other}"),
        };
        compare(&result, &row["result"], &format!("result/{i}"));
        let mut policy_state = serde_json::to_value(policy).unwrap();
        policy_state["cameras"] = serde_json::to_value(&nav.camera_policy).unwrap();
        compare(&policy_state, &row["after"], &format!("policy/{i}"));
        compare(
            &serde_json::to_value(output).unwrap(),
            &row["output_after"],
            &format!("output/{i}"),
        );
        compare(&events(&nav), &row["events_after"], &format!("events/{i}"));
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 2400);
    std::fs::remove_dir_all(path).unwrap();
}
