use openpilot_radarcan::wire;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
struct Row {
    kind: String,
    bytes: Vec<u8>,
    expected: Value,
}
#[test]
fn context_decode_matches_actual_cereal_readers() {
    let rows: Vec<Row> = serde_json::from_slice(
        &zstd::stream::decode_all(&include_bytes!("fixtures/bosch-wire-final.json.zst")[..])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 512);
    for (index, row) in rows.into_iter().enumerate() {
        let actual = if row.kind == "pose" {
            let message = wire::bosch_pose(&row.bytes).unwrap();
            json!({"timestamp":message.timestamp,"value":message.value.as_deref()})
        } else {
            let message = wire::bosch_model(&row.bytes).unwrap();
            let value = message.value.map(|input| {
                let mut value = serde_json::to_value(&input.geometry).unwrap();
                value["leadsV3"] = json!(input
                    .leads
                    .iter()
                    .map(|lead| { json!({"x":lead.x,"y":lead.y,"prob":lead.prob}) })
                    .collect::<Vec<_>>());
                value
            });
            json!({"timestamp":message.timestamp,"value":value})
        };
        assert_eq!(actual, row.expected, "Cereal context row {index}");
        if row.kind == "pose" {
            assert!(matches!(
                wire::bosch_model(&row.bytes),
                Err(wire::Error::Event("modelV2"))
            ));
        } else {
            assert!(matches!(
                wire::bosch_pose(&row.bytes),
                Err(wire::Error::Event("livePose"))
            ));
        }
    }
}
#[test]
fn context_decoder_rejects_overflowed_publication_timestamps() {
    use capnp::message::Builder;
    use openpilot_cereal::log_capnp::event;
    for model in [false, true] {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<event::Builder>();
        root.set_log_mono_time(u64::MAX);
        if model {
            root.init_model_v2();
        } else {
            root.init_live_pose();
        }
        let bytes = capnp::serialize::write_message_to_words(&message);
        let result = if model {
            wire::bosch_model(&bytes).map(|_| ())
        } else {
            wire::bosch_pose(&bytes).map(|_| ())
        };
        assert!(matches!(result, Err(wire::Error::Count(_))));
    }
}
