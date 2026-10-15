use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::Model,
    context::{Cache, Message, Pose},
    model_snapshot::Input,
    provider::{Config, Provider},
    road_edge::{GeometryKey, RoadEdge},
};
use serde_json::{json, Value};
use std::sync::Arc;
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{e}: {v}"))
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                assert_eq!(a, b, "{path}");
            } else if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                assert_eq!(a, b, "{path}");
            } else {
                assert_eq!(
                    a.as_f64().unwrap().to_bits(),
                    b.as_f64().unwrap().to_bits(),
                    "{path}: {a} != {b}"
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn key_json(v: &GeometryKey) -> Value {
    fn axis(v: &[u64]) -> Value {
        let s: String = v
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .map(|v| format!("{v:02x}"))
            .collect();
        json!({"source_bytes_hex":s})
    }
    json!(v
        .iter()
        .map(|(x, y)| json!([axis(x), axis(y)]))
        .collect::<Vec<_>>())
}
fn compare_state(s: &RoadEdge, v: &Value, path: &str) {
    let mut expected = v.clone();
    expected.as_object_mut().unwrap().remove("_validated_lines");
    expected.as_object_mut().unwrap().remove("_last_ingest");
    compare(
        &json!({"enabled":s.enabled,"model_edges":s.model_edges,"publication_model_edges":s.publication_model_edges,"last_ns":s.last_ns,"source_model_ns":s.source_model_ns,"would_suppress":s.would_suppress,"publication_suppressed":s.publication_suppressed}),
        &expected,
        path,
    );
    let validated = s
        .validated
        .as_ref()
        .map(|v| json!([key_json(&v.key), v.lines]));
    compare(
        &json!(validated),
        &v["_validated_lines"],
        &format!("{path}/validated"),
    );
    if let Some(ingest) = &s.last_ingest {
        let expected = &v["_last_ingest"];
        assert!(!expected.is_null(), "{path}/last-ingest");
        assert_eq!(ingest.timestamp, expected[1].as_i64().unwrap());
        assert_eq!(ingest.invalid, expected[2].as_bool().unwrap());
        let actual = ingest.model.as_ref().map(|m| json!([m.edges, m.edge_stds]));
        let expected: Option<Model> = decode(&expected[0]);
        let expected = expected.map(|m| json!([m.edges, m.edge_stds]));
        compare(
            &json!(actual),
            &json!(expected),
            &format!("{path}/model-input"),
        );
    } else {
        assert!(v["_last_ingest"].is_null());
    }
}
fn remove_slot_tags(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("source_slots_type");
            for value in m.values_mut() {
                remove_slot_tags(value);
            }
        }
        Value::Array(a) => {
            for v in a {
                remove_slot_tags(v);
            }
        }
        _ => {}
    }
}
#[test]
fn causal_context_identity_snapshot_hold_and_reset_match_source() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-context-final.json.zst").as_slice(),
    )
    .unwrap();
    let source: Value = serde_json::from_slice(&bytes).unwrap();
    let poses: Vec<Arc<Pose>> = decode::<Vec<Pose>>(&source["poses"])
        .into_iter()
        .map(Arc::new)
        .collect();
    let models: Vec<Arc<Input>> = decode::<Vec<Input>>(&source["models"])
        .into_iter()
        .map(Arc::new)
        .collect();
    let mut cache = Cache::default();
    let mut provider = Provider::new(Config::default()).unwrap();
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1536);
    for (i, row) in rows.iter().enumerate() {
        if row["reset"].as_bool().unwrap() {
            provider.b5_birth_defer.reset("TEST_RESET_SAME_HELD_MODEL");
        }
        let input = &row["input"];
        let pose = Message {
            value: input["pose_id"]
                .as_u64()
                .map(|id| Arc::clone(&poses[id as usize])),
            timestamp: decode(&input["pose_ns"]),
        };
        let model = Message {
            value: input["model_id"]
                .as_u64()
                .map(|id| Arc::clone(&models[id as usize])),
            timestamp: decode(&input["model_ns"]),
        };
        let pending = cache.set(&mut provider, decode(&input["now_ns"]), pose, model);
        let pose_id = |p: &Arc<Pose>| poses.iter().position(|v| Arc::ptr_eq(v, p)).unwrap();
        let model_id = |p: &Arc<Input>| models.iter().position(|v| Arc::ptr_eq(v, p)).unwrap();
        let mut actual = json!({"context":[pending.now,pending.yaw,pending.cues,pending.path,pending.path_ns,pending.path_source_ns],"last_pose":[cache.last_pose.value.as_ref().map(pose_id),cache.last_pose.timestamp],"last_model":[cache.last_model.value.as_ref().map(model_id),cache.last_model.timestamp],"path_ns":cache.path_ns,"path_source_ns":cache.path_source_ns,"path":cache.path,"cached_pose":cache.cached_pose().map(|(p,y)|(pose_id(p),y)),"cached_models":cache.cached_models().into_iter().map(|(p,ns,s)|json!([model_id(p),ns,s])).collect::<Vec<_>>(),"b5":provider.b5_birth_defer,"m3":provider.mirror_m3_shadow});
        let mut expected = row["after"].clone();
        let road = expected.as_object_mut().unwrap().remove("road").unwrap();
        compare_state(
            &provider.road_edge_filter,
            &road,
            &format!("context/{i}/road"),
        );
        remove_slot_tags(&mut expected);
        remove_slot_tags(&mut actual);
        compare(&actual, &expected, &format!("context/{i}"));
    }
}
