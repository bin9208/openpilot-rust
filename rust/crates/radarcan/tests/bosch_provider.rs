use openpilot_radarcan::decoder::hyundai::bosch::{
    birth_b5::Model,
    physical::{Object, VisionCue},
    provider::{CanFrame, Config, Context, Message, Packet, Provider},
    road_edge::{GeometryKey, Ingest, RoadEdge, Validated},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    sync::Arc,
};
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
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => v.as_f64().unwrap(),
    }
}
fn key(v: &Value) -> GeometryKey {
    v.as_array()
        .unwrap()
        .iter()
        .map(|line| {
            fn axis(v: &Value) -> Vec<u64> {
                let s = v["source_bytes_hex"].as_str().unwrap().as_bytes();
                let bytes: Vec<u8> = s
                    .chunks_exact(2)
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                    .collect();
                bytes
                    .chunks_exact(8)
                    .map(|p| u64::from_le_bytes(p.try_into().unwrap()))
                    .collect()
            }
            (axis(&line[0]), axis(&line[1]))
        })
        .collect()
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
fn restore_road(v: &Value) -> RoadEdge {
    let mut s = RoadEdge::new(decode(&v["enabled"]));
    s.model_edges = decode(&v["model_edges"]);
    s.publication_model_edges = decode(&v["publication_model_edges"]);
    s.last_ns = decode(&v["last_ns"]);
    s.source_model_ns = decode(&v["source_model_ns"]);
    s.would_suppress = decode(&v["would_suppress"]);
    s.publication_suppressed = decode(&v["publication_suppressed"]);
    if !v["_validated_lines"].is_null() {
        s.validated = Some(Validated {
            key: key(&v["_validated_lines"][0]),
            lines: decode(&v["_validated_lines"][1]),
        });
    }
    if !v["_last_ingest"].is_null() {
        let cache = &v["_last_ingest"];
        s.last_ingest = Some(Ingest {
            model: decode::<Option<Model>>(&cache[0]).map(Arc::new),
            timestamp: decode(&cache[1]),
            invalid: decode(&cache[2]),
        });
    }
    s
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
fn bytes(v: &Value) -> Vec<u8> {
    let s = v["source_bytes_hex"].as_str().unwrap().as_bytes();
    s.chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn frames(v: &Value) -> Vec<CanFrame> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|f| CanFrame {
            timestamp_ns: decode(&f["timestamp_ns"]),
            address: decode(&f["address"]),
            payload: bytes(&f["payload"]),
            order: decode(&f["order"]),
        })
        .collect()
}
fn frame_json(f: &CanFrame) -> Value {
    json!({"timestamp_ns":f.timestamp_ns,"address":f.address,"payload":{"source_bytes_hex":f.payload.iter().map(|v|format!("{v:02x}")).collect::<String>()},"order":f.order})
}
fn triple<T: serde::de::DeserializeOwned>(v: &Value) -> BTreeMap<(i64, u32, u32), T> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            let a: Vec<_> = k
                .trim_matches(['(', ')'])
                .split(',')
                .map(str::trim)
                .collect();
            (
                (
                    a[0].parse().unwrap(),
                    a[1].parse().unwrap(),
                    a[2].parse().unwrap(),
                ),
                decode(v),
            )
        })
        .collect()
}
fn triple_json<T: serde::Serialize>(v: &BTreeMap<(i64, u32, u32), T>) -> Value {
    Value::Object(
        v.iter()
            .map(|(&(a, b, c), v)| (format!("({a}, {b}, {c})"), json!(v)))
            .collect(),
    )
}
fn restore(v: &Value) -> Provider {
    let mut config: Config =
        decode(&json!({"bus":v["bus"],"camera_bus":v["camera_bus"],"scc_bus":v["scc_bus"]}));
    config.qualification = !v["qualifier"].is_null();
    config.camera_extended_mode = decode(&v["camera_extended"]["mode"]);
    config.p91_mode = decode(&v["p91"]["mode"]);
    config.oem_gate_mode = decode(&v["oem_gate"]["mode"]);
    config.curve_reacquire_mode = decode(&v["camera_extended"]["curve_reacquire_mode"]);
    config.family_companion_mode = decode(&v["family_companion"]["mode"]);
    config.burst_multireturn_mode = decode(&v["burst_multireturn"]["mode"]);
    config.b5_mode = decode(&v["b5_birth_defer"]["mode"]);
    config.sidepass_lateral_mode = decode(&v["sidepass_lateral"]["mode"]);
    config.mirror_birth_mode = decode(&v["mirror_birth_hold"]["mode"]);
    let mut s = Provider::new(config).unwrap();
    s.tracker = decode(&v["tracker"]);
    s.camera_extended = decode(&v["camera_extended"]);
    s.family_companion = decode(&v["family_companion"]);
    s.burst_multireturn = decode(&v["burst_multireturn"]);
    s.b5_birth_defer = decode(&v["b5_birth_defer"]);
    s.mirror_birth_hold = decode(&v["mirror_birth_hold"]);
    s.mirror_m3_shadow = decode(&v["mirror_m3_shadow"]);
    s.unsupported_birth = decode(&v["unsupported_birth"]);
    s.sidepass_lateral = decode(&v["sidepass_lateral"]);
    s.p91 = decode(&v["p91"]);
    s.oem_gate = decode(&v["oem_gate"]);
    s.scc_obj_valid = decode(&v["scc_obj_valid"]);
    s.scc_obj_valid_ns = decode(&v["scc_obj_valid_ns"]);
    s.can_error = decode(&v["can_error"]);
    s.wrong_config = decode(&v["wrong_config"]);
    s.last_scan_timestamp_ns = decode(&v["last_scan_timestamp_ns"]);
    s.test_last_suppressed = decode(&v["test_last_suppressed"]);
    s.test_last_active_groups = decode(&v["test_last_active_groups"]);
    s.last_oem_state = decode(&v["last_oem_state"]);
    s.companion_deferred_points = decode(&v["companion_deferred_points"]);
    s.companion_defer_scans = decode(&v["companion_defer_scans"]);
    s.last_companion_deferred = decode(&v["last_companion_deferred"]);
    s.last_oem_slot = decode(&v["last_oem_slot"]);
    s.oem_nearer_corrections = decode(&v["oem_nearer_corrections"]);
    s.oem_nearer_scans = decode(&v["oem_nearer_scans"]);
    s.last_oem_nearer = decode(&v["last_oem_nearer"]);
    s.unsupported_context = decode(&v["_unsupported_birth_context"]);
    s.anchors = decode(&v["_anchors"]);
    s.order = decode(&v["_order"]);
    s.start_ns = decode(&v["_start_ns"]);
    s.last_now_ns = decode(&v["_last_now_ns"]);
    s.last_closed_anchor_ns = decode(&v["_last_closed_anchor_ns"]);
    s.last_output_ns = decode(&v["_last_output_ns"]);
    s.pending_error = decode(&v["_pending_error"]);
    s.companion_ns = decode(&v["_companion_ns"]);
    s.companion_prev_ns = decode(&v["_companion_prev_ns"]);
    s.companion_hidden = decode(&v["_companion_hidden"]);
    s.frames = frames(&v["_frames"]);
    s.companion_pairs = triple(&v["companion_pairs"]);
    s.companion_shared_start = triple(&v["_companion_shared_start"]);
    s.road_edge_filter = restore_road(&v["road_edge_filter"]);
    s.camera = decode(&v["camera_extended"]["camera"]);
    s.publication_aliases.physical_to_alias =
        decode(&v["publication_aliases"]["physical_to_alias"]);
    s.publication_aliases.alias_to_physical =
        decode(&v["publication_aliases"]["alias_to_physical"]);
    s.publication_aliases.last_published_ns =
        decode(&v["publication_aliases"]["last_published_ns"]);
    s.publication_aliases.free_aliases = decode(&v["publication_aliases"]["free_aliases"]);
    s.publication_aliases.peak_usage = decode(&v["publication_aliases"]["peak_usage"]);
    s.publication_aliases.denial_count = decode(&v["publication_aliases"]["denial_count"]);
    s.publication_aliases.grace_eviction_count =
        decode(&v["publication_aliases"]["grace_eviction_count"]);
    s.debug.objects = decode(&v["_debug_objects"]);
    s.debug.phase_ns = decode(&v["_debug_phase_ns"]);
    s.debug.closed_ns = decode(&v["_debug_closed_ns"]);
    s.debug.tick = decode(&v["_debug_tick"]);
    s.debug.complete = decode(&v["_debug_complete"]);
    s.debug.oem_word = decode(&v["_debug_oem_word"]);
    s.debug.oem_slot = decode(&v["_debug_oem_slot"]);
    s.debug.oem_matches = decode(&v["_debug_oem_matches"]);
    s.debug.processed_word = decode(&v["_debug_processed_word"]);
    s.debug.processed_pids = decode(&v["_debug_processed_pids"]);
    s.debug.word0_active = decode(&v["_debug_word0_active"]);
    s.debug.word1_active = decode(&v["_debug_word1_active"]);
    s.debug.oem_state = decode(&v["_debug_oem_state"]);
    s.debug.gate_suppress = decode(&v["_debug_gate_suppress"]);
    s.debug.gate_reasons = decode(&v["_debug_gate_reasons"]);
    s.debug.oem_intent = decode(&v["_debug_oem_intent"]);
    s.debug.timeout = decode(&v["_debug_timeout"]);
    s
}
fn state(s: &Provider, expected: &Value, path: &str) {
    let mut actual = serde_json::Map::new();
    actual.insert("tracker".into(), json!(s.tracker));
    actual.insert("camera_extended".into(), json!(s.camera_extended));
    actual.insert("family_companion".into(), json!(s.family_companion));
    actual.insert("burst_multireturn".into(), json!(s.burst_multireturn));
    actual.insert("b5_birth_defer".into(), json!(s.b5_birth_defer));
    actual.insert("mirror_birth_hold".into(), json!(s.mirror_birth_hold));
    actual.insert("mirror_m3_shadow".into(), json!(s.mirror_m3_shadow));
    actual.insert("unsupported_birth".into(), json!(s.unsupported_birth));
    actual.insert("sidepass_lateral".into(), json!(s.sidepass_lateral));
    actual.insert("p91".into(), json!(s.p91));
    actual.insert("oem_gate".into(), json!(s.oem_gate));
    actual.insert("scc_obj_valid".into(), json!(s.scc_obj_valid));
    actual.insert("scc_obj_valid_ns".into(), json!(s.scc_obj_valid_ns));
    actual.insert("can_error".into(), json!(s.can_error));
    actual.insert("wrong_config".into(), json!(s.wrong_config));
    actual.insert(
        "last_scan_timestamp_ns".into(),
        json!(s.last_scan_timestamp_ns),
    );
    actual.insert("test_last_suppressed".into(), json!(s.test_last_suppressed));
    actual.insert(
        "test_last_active_groups".into(),
        json!(s.test_last_active_groups),
    );
    actual.insert("last_oem_state".into(), json!(s.last_oem_state));
    actual.insert(
        "companion_deferred_points".into(),
        json!(s.companion_deferred_points),
    );
    actual.insert(
        "companion_defer_scans".into(),
        json!(s.companion_defer_scans),
    );
    actual.insert(
        "last_companion_deferred".into(),
        json!(s.last_companion_deferred),
    );
    actual.insert("last_oem_slot".into(), json!(s.last_oem_slot));
    actual.insert(
        "oem_nearer_corrections".into(),
        json!(s.oem_nearer_corrections),
    );
    actual.insert("oem_nearer_scans".into(), json!(s.oem_nearer_scans));
    actual.insert("last_oem_nearer".into(), json!(s.last_oem_nearer));
    actual.insert(
        "_unsupported_birth_context".into(),
        json!(s.unsupported_context),
    );
    actual.insert("_anchors".into(), json!(s.anchors));
    actual.insert("_order".into(), json!(s.order));
    actual.insert("_start_ns".into(), json!(s.start_ns));
    actual.insert("_last_now_ns".into(), json!(s.last_now_ns));
    actual.insert(
        "_last_closed_anchor_ns".into(),
        json!(s.last_closed_anchor_ns),
    );
    actual.insert("_last_output_ns".into(), json!(s.last_output_ns));
    actual.insert("_pending_error".into(), json!(s.pending_error));
    actual.insert("_companion_ns".into(), json!(s.companion_ns));
    actual.insert("_companion_prev_ns".into(), json!(s.companion_prev_ns));
    actual.insert("_companion_hidden".into(), json!(s.companion_hidden));
    actual.insert("_debug_objects".into(), json!(s.debug.objects));
    actual.insert("_debug_phase_ns".into(), json!(s.debug.phase_ns));
    actual.insert("_debug_closed_ns".into(), json!(s.debug.closed_ns));
    actual.insert("_debug_tick".into(), json!(s.debug.tick));
    actual.insert("_debug_complete".into(), json!(s.debug.complete));
    actual.insert("_debug_oem_word".into(), json!(s.debug.oem_word));
    actual.insert("_debug_oem_slot".into(), json!(s.debug.oem_slot));
    actual.insert("_debug_oem_matches".into(), json!(s.debug.oem_matches));
    actual.insert(
        "_debug_processed_word".into(),
        json!(s.debug.processed_word),
    );
    actual.insert(
        "_debug_processed_pids".into(),
        json!(s.debug.processed_pids),
    );
    actual.insert("_debug_word0_active".into(), json!(s.debug.word0_active));
    actual.insert("_debug_word1_active".into(), json!(s.debug.word1_active));
    actual.insert("_debug_oem_state".into(), json!(s.debug.oem_state));
    actual.insert("_debug_gate_suppress".into(), json!(s.debug.gate_suppress));
    actual.insert("_debug_gate_reasons".into(), json!(s.debug.gate_reasons));
    actual.insert("_debug_oem_intent".into(), json!(s.debug.oem_intent));
    actual.insert("_debug_timeout".into(), json!(s.debug.timeout));

    actual.insert("bus".into(), json!(s.config.bus));
    actual.insert("camera_bus".into(), json!(s.config.camera_bus));
    actual.insert("scc_bus".into(), json!(s.config.scc_bus));
    actual.insert(
        "qualifier".into(),
        if s.config.qualification {
            json!({})
        } else {
            Value::Null
        },
    );
    actual.insert("publication_aliases".into(), json!(s.publication_aliases));
    actual
        .get_mut("camera_extended")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("camera".into(), json!(s.camera));
    actual.insert(
        "_frames".into(),
        json!(s.frames.iter().map(frame_json).collect::<Vec<_>>()),
    );
    actual.insert("companion_pairs".into(), triple_json(&s.companion_pairs));
    actual.insert(
        "_companion_shared_start".into(),
        triple_json(&s.companion_shared_start),
    );
    let mut expected = expected.clone();
    let road = expected
        .as_object_mut()
        .unwrap()
        .remove("road_edge_filter")
        .unwrap();
    compare_state(&s.road_edge_filter, &road, &format!("{path}/road"));
    compare(&Value::Object(actual), &expected, path);
}
#[test]
fn complete_provider_receive_and_publication_match_canonical_pinned_callbacks() {
    let mut counts = [0usize; 2];
    let mut typed_exclusions = 0;
    for (mode, compressed) in [
        include_bytes!("fixtures/bosch-provider-update-final.jsonl.zst").as_slice(),
        include_bytes!("fixtures/bosch-provider-publication-final.jsonl.zst").as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        for (i, line) in BufReader::new(zstd::stream::read::Decoder::new(compressed).unwrap())
            .lines()
            .enumerate()
        {
            let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let before = &row["before"];
            // Source tests sometimes use None as a key-only marker for a live state.
            // A valid native runtime state has its complete typed observation; those
            // foreign-value probes and injected camera snapshot stand-ins remain
            // separately identified, never fabricated here.
            if before["tracker"]["group_manager"]["states"]
                .as_object()
                .unwrap()
                .values()
                .any(Value::is_null)
                || (before["camera_extended"]["camera"].is_object()
                    && before["camera_extended"]["camera"]
                        .get("_counter")
                        .is_none())
            {
                typed_exclusions += 1;
                continue;
            }
            let mut s = restore(before);
            let input = &row["input"];
            let result = execute(&mut s, input, mode);
            match result {
                Ok(v) => compare(&v, &row["result"], &format!("{mode}/{i}/result")),
                Err(e) => assert_eq!(
                    Some(e.as_str()),
                    row["error"]["message"].as_str(),
                    "{mode}/{i}/error"
                ),
            }
            state(&s, &row["after"], &format!("{mode}/{i}/state"));
            counts[mode] += 1;
        }
    }
    assert_eq!(typed_exclusions, 170);
    assert_eq!(counts.iter().sum::<usize>(), 576);
}

fn execute(s: &mut Provider, input: &Value, mode: usize) -> Result<Value, String> {
    if mode == 0 {
        let packets: Vec<Packet> = input["can_packets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Packet {
                timestamp_ns: decode(&p[0]),
                messages: p[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| Message {
                        address: decode(&m[0]),
                        payload: bytes(&m[1]),
                        bus: decode(&m[2]),
                    })
                    .collect(),
            })
            .collect();
        let vision: Vec<VisionCue> = decode(&input["vision"]);
        let path: Vec<(f64, f64)> = decode(&input["path"]);
        s.update(
            &packets,
            Context {
                now: decode(&input["now_ns"]),
                ego: number(&input["v_ego"]),
                yaw: input["yaw_rate_left"].as_f64(),
                vision: &vision,
                path: &path,
                path_ns: decode(&input["path_ns"]),
                path_source_ns: decode(&input["path_source_ns"]),
            },
        )
        .map(|v| json!(v))
    } else {
        let objects: Vec<Object> = decode(&input["objects"]);
        s.publication_view(&objects, decode(&input["timestamp_ns"]))
            .map(|v| json!(v))
    }
}

#[test]
fn causal_provider_sequences_and_constructor_modes_match_pinned_source() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/bosch-provider-constructors.json")).unwrap();
    assert_eq!(rows.len(), 46);
    for (i, row) in rows.iter().enumerate() {
        let result = Provider::new(decode(&row["config"]));
        if let Some(error) = row.get("error") {
            let e = match result {
                Ok(_) => panic!("constructor {i} should fail"),
                Err(e) => e,
            };
            assert_eq!(e, error["message"].as_str().unwrap(), "constructor/{i}");
        } else {
            state(&result.unwrap(), &row["after"], &format!("constructor/{i}"));
        }
    }
    let compressed = include_bytes!("fixtures/bosch-provider-sequences-final.jsonl.zst");
    let mut current = None;
    let mut provider = None;
    let mut counts = [0usize; 2];
    for (i, line) in
        BufReader::new(zstd::stream::read::Decoder::new(compressed.as_slice()).unwrap())
            .lines()
            .enumerate()
    {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let case = row["case"].as_u64().unwrap();
        if current != Some(case) {
            provider = Some(Provider::new(decode(&row["config"])).unwrap());
            current = Some(case);
        }
        let s = provider.as_mut().unwrap();
        let mode = usize::from(row["op"] == "publication_view");
        state(s, &row["before"], &format!("sequence/{i}/before"));
        let result =
            execute(s, &row["input"], mode).unwrap_or_else(|e| panic!("sequence/{i}: {e}"));
        compare(&result, &row["result"], &format!("sequence/{i}/result"));
        state(s, &row["after"], &format!("sequence/{i}/after"));
        compare(
            &s.debug_snapshot(),
            &row["debug_snapshot"],
            &format!("sequence/{i}/debug"),
        );
        counts[mode] += 1;
    }
    assert_eq!(counts, [2560, 224]);
}
