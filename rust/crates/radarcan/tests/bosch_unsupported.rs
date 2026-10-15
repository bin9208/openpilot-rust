use indexmap::IndexMap;
use openpilot_radarcan::decoder::hyundai::bosch::{
    camera::CameraObject,
    unsupported_birth::{Context, Decision, Frame, Observation, Snapshot, UnsupportedBirth},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Deserialize)]
struct OwnedSnapshot {
    objects: Vec<CameraObject>,
    count: usize,
    complete: i64,
}
#[derive(Deserialize)]
struct Input {
    enabled: bool,
    can_error: bool,
    wrong_config: bool,
    publication: Option<i64>,
    scan: Option<i64>,
    context: Option<Context>,
    identity: Option<(i64, usize, bool)>,
    extended_ns: Option<i64>,
    association: Option<(i32, i64, i64)>,
    processed: bool,
    snapshot: Option<OwnedSnapshot>,
    #[serde(rename = "object")]
    observation: Observation,
    first_only: bool,
    require_single_candidate: bool,
}
#[test]
fn unsupported_birth_verdict_and_publication_all_source_reasons_and_freshness_boundaries() {
    let bytes = zstd::stream::decode_all(
        include_bytes!("fixtures/bosch-unsupported-birth-final.json.zst").as_slice(),
    )
    .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 2141);
    let mut excludes = 0;
    let mut reasons = BTreeSet::new();
    for (i, row) in rows.iter().enumerate() {
        let input: Input = serde_json::from_value(row["input"].clone()).unwrap();
        let identity: BTreeMap<_, _> = input.identity.into_iter().map(|v| (1, v)).collect();
        let associations: IndexMap<_, _> = input.association.into_iter().map(|v| (1, v)).collect();
        let processed = if input.processed {
            BTreeSet::from([1])
        } else {
            BTreeSet::new()
        };
        let frame = Frame {
            can_error: input.can_error,
            wrong_config: input.wrong_config,
            context: input.context.as_ref(),
            scan: input.scan,
            identity: &identity,
            extended_ns: input.extended_ns,
            associations: &associations,
            processed: &processed,
            snapshot: input.snapshot.as_ref().map(|s| Snapshot {
                objects: &s.objects,
                count: s.count,
                complete: s.complete,
            }),
        };
        let mut state = UnsupportedBirth::new(input.enabled);
        let value = state
            .verdict(
                &frame,
                &input.observation,
                input.publication,
                input.first_only,
                input.require_single_candidate,
            )
            .unwrap();
        reasons.insert(value.reason.clone());
        excludes += usize::from(value.exclude);
        compare(&json!(value), &row["result"], &format!("verdict/{i}"));
        state.last_decisions.insert(
            99,
            Decision {
                exclude: true,
                reason: "OLD".into(),
                details: None,
            },
        );
        let objects: Vec<Observation> =
            serde_json::from_value(row["publication_objects"].clone()).unwrap();
        let indices = state
            .publication_indices(&frame, &objects, input.publication)
            .unwrap();
        let ids: Vec<_> = indices
            .into_iter()
            .map(|i| objects[i].physical_track_id)
            .collect();
        compare(
            &json!(ids),
            &row["publication_ids"],
            &format!("publication/{i}"),
        );
        compare(
            &json!(state.last_decisions),
            &row["last_decisions"],
            &format!("decisions/{i}"),
        );
    }
    assert_eq!(reasons.len(), 18);
    assert_eq!(excludes, 222);
}
