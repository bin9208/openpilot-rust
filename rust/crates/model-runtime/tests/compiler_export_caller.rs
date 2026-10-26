#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::compiler_schedule::export::{prepare, Request};
use serde_json::{json, Value};

fn storage(base: u64, offset: usize, bytes: usize) -> Value {
    json!({"base":base,"allocation_bytes":8,"offset":offset,"bytes":bytes,"device":"QCOM","allocated":false})
}
fn request() -> Value {
    json!({"version":1,"initialized":[{"key":10,"data":"0102030405060708"}],
        "events":[{"op":"view","buffer":storage(20,0,8)},
                  {"op":"write","buffer":storage(10,0,4)},
                  {"op":"read","buffer":storage(10,0,8)},
                  {"op":"write","buffer":storage(20,0,8)},
                  {"op":"read","buffer":storage(20,0,8)}],
        "programs":[{"source":"get_local_id(0)","binary":""}],
        "launches":[{"program":0,"global":[{"type":"integer","value":"64"},{"type":"integer","value":"1"},{"type":"integer","value":"1"}],"local":null}],
        "emitted_by_source":[0,1],"stage_ends":[{"name":"prepare","end":0},{"name":"model","end":1}]})
}
fn plan(value: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
    serde_json::to_value(prepare(request)?).map_err(|e| e.to_string())
}
#[test]
fn live_wire_composes_alias_weights_launch_and_stage_cores() {
    let result = plan(request()).unwrap();
    assert_eq!(result["weights"], "0000000005060708");
    assert_eq!(
        result["allocations"],
        json!([{"bytes":8,"weight_offset":null},{"bytes":8,"weight_offset":0}])
    );
    assert_eq!(result["event_views"], json!([0, 1, 2, 0, 0]));
    assert_eq!(
        result["entrypoints"],
        json!([{"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":1}])
    );
    assert_eq!(
        result["launches"][0]["local"],
        json!(vec![json!({"type":"integer","value":"1"}); 3])
    );
}
#[test]
fn live_wire_rejects_lazy_missing_bytes_bad_alias_and_allocated_qcom() {
    for mode in 0..5 {
        let mut value = request();
        match mode {
            0 => value["initialized"][0]["key"] = json!(99),
            1 => value["initialized"][0]["data"] = json!("00"),
            2 => value["events"][2]["buffer"]["allocation_bytes"] = json!(9),
            3 => value["events"][0]["buffer"]["allocated"] = json!(true),
            _ => value["stage_ends"][1]["end"] = json!(2),
        }
        assert!(plan(value).is_err(), "mode {mode}");
    }
    let mut value = request();
    value["events"][1]["buffer"] = storage(10, 0, 8);
    value["initialized"] = json!([]);
    assert_eq!(plan(value).unwrap()["weights"], "");
}

#[test]
fn existing_cli_exports_real_plan_and_rejects_invalid_request_without_output() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for valid in [true, false] {
        let mut value = request();
        if !valid {
            value["version"] = json!(99);
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
            .args(["--library", "/unused-export-plan-provider", "--export-plan"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        if valid {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&result.stdout).unwrap(),
                plan(value).unwrap()
            );
        } else {
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
        }
    }
}

#[test]
fn existing_cli_preserves_explicit_launch_float_bits_and_dimension_types() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    const GLOBAL: f64 = 102.49635675101081;
    const LOCAL: f64 = 51.248178375505404;
    // Keep the decimal on the wire: an in-memory prepare() comparison alone
    // would miss serde_json's default parser changing the admitted f64 bits.
    // Explicit locals bypass regrouping in the current Rust/Python launch core.
    let request = br#"{"version":1,"initialized":[],"events":[],
        "programs":[{"source":"get_local_id(0)","binary":""}],
        "launches":[{"program":0,
            "global":[{"type":"float","value":102.49635675101081},{"type":"integer","value":"2"},{"type":"boolean","value":true}],
            "local":[{"type":"float","value":51.248178375505404},{"type":"float","value":1.0},{"type":"integer","value":"1"}]}],
        "emitted_by_source":[0,1],"stage_ends":[{"name":"model","end":1}]}"#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
        .args(["--library", "/unused-export-plan-provider", "--export-plan"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(request).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::str::from_utf8(&result.stdout)
            .unwrap()
            .contains("\"value\":51.248178375505404"),
        "CLI changed the regression decimal before response decoding"
    );
    let decoded: Value = serde_json::from_slice(&result.stdout).unwrap();
    let launch = &decoded["launches"][0];
    assert_eq!(
        launch,
        &json!({"global":[{"type":"float","value":GLOBAL},{"type":"integer","value":"2"},{"type":"boolean","value":true}],
            "local":[{"type":"float","value":LOCAL},{"type":"float","value":1.0},{"type":"integer","value":"1"}]})
    );
    for (dimension, expected) in [
        (&launch["global"][0], GLOBAL),
        (&launch["local"][0], LOCAL),
        (&launch["local"][1], 1.0_f64),
    ] {
        assert_eq!(dimension["type"], "float");
        assert!(dimension["value"].is_f64());
        assert_eq!(
            dimension["value"].as_f64().unwrap().to_bits(),
            expected.to_bits()
        );
    }
}

fn request_v2() -> Value {
    let mut value = request();
    value.as_object_mut().unwrap().remove("stage_ends");
    value["version"] = json!(2);
    value["retained_calls"] = json!([100]);
    value["stages"] = json!([{"name":"prepare","calls":[]},{"name":"model","calls":[100]}]);
    value
}

#[test]
fn raw_v2_preserves_pruning_repeated_occurrences_and_emission_positions() {
    let mut value = request_v2();
    value["retained_calls"] = json!([11, 11, 13]);
    value["emitted_by_source"] = json!([0, 2, 2, 5]);
    value["stages"] = json!([
        {"name":"prepare","calls":[10,11]},
        {"name":"policy","calls":[11,12,13]},
        {"name":"empty","calls":[]}
    ]);
    let result = plan(value).unwrap();
    // Original set membership counts every occurrence; source IDs and unique
    // kernel IDs are not emitted positions. These ranges are fixed inputs.
    assert_eq!(result["version"], 2);
    assert_eq!(
        result["entrypoints"],
        json!([
            {"name":"prepare","start":0,"end":2},
            {"name":"policy","start":2,"end":5},
            {"name":"empty","start":5,"end":5}
        ])
    );
    assert_eq!(result["weights"], "0000000005060708");
    assert_eq!(result["event_views"], json!([0, 1, 2, 0, 0]));
    for staged in [false, true] {
        let mut value = request_v2();
        value["launches"] = json!([]);
        value["retained_calls"] = json!([]);
        value["emitted_by_source"] = json!([0]);
        value["stages"] = if staged {
            json!([{"name":"pruned","calls":[100]}])
        } else {
            json!([])
        };
        let result = plan(value).unwrap();
        assert_eq!(result["version"], if staged { 2 } else { 1 });
        assert_eq!(
            result["entrypoints"],
            if staged {
                json!([{"name":"pruned","start":0,"end":0}])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn raw_v2_rejects_cross_version_null_unknown_and_malformed_stage_contracts() {
    for mode in 0..13 {
        let mut value = request_v2();
        match mode {
            0 => value["stage_ends"] = json!([]),
            1 => value["stage_ends"] = Value::Null,
            2 => value["stages"] = Value::Null,
            3 => value["retained_calls"] = Value::Null,
            4 => {
                value.as_object_mut().unwrap().remove("stages");
            }
            5 => {
                value.as_object_mut().unwrap().remove("retained_calls");
            }
            6 => value["version"] = json!(1),
            7 => value["stages"][1]["unexpected"] = json!(true),
            8 => value["stages"][1]["calls"] = json!([true]),
            9 => value["retained_calls"] = json!([100.0]),
            10 => value["emitted_by_source"] = json!([0, 1, 2]),
            11 => value["stages"][1]["calls"] = json!([999]),
            _ => value["stages"][1]["name"] = json!("prepare"),
        }
        assert!(plan(value).is_err(), "v2 mode {mode}");
    }
    for extra in [json!([]), Value::Null] {
        let mut value = request();
        value["stages"] = extra;
        assert!(plan(value).is_err(), "v1 must reject v2 fields");
    }
    let mut value = request_v2();
    value["launches"] = json!([]);
    value["retained_calls"] = json!([]);
    value["emitted_by_source"] = json!([0]);
    value["stages"] = json!((0..65536)
        .map(|i| json!({"name":format!("stage-{i}"),"calls":[]}))
        .collect::<Vec<_>>());
    assert_eq!(
        plan(value.clone()).unwrap()["entrypoints"]
            .as_array()
            .unwrap()
            .len(),
        65536
    );
    value["stages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"too-many","calls":[]}));
    assert!(plan(value).is_err());
    let mut value = request_v2();
    value["stages"][1]["calls"] = json!(vec![100u64; 1_048_577]);
    assert!(plan(value).is_err());
}

#[test]
fn existing_cli_accepts_raw_v2_and_rejects_bad_stage_without_provider() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    for valid in [true, false] {
        let mut request = request_v2();
        if !valid {
            request["stages"][1]["unexpected"] = json!(true);
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
            .args(["--library", "/unused-export-plan-provider", "--export-plan"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        if valid {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["version"], 2);
            assert_eq!(result["weights"], "0000000005060708");
            assert_eq!(
                result["entrypoints"],
                json!([
                    {"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":1}
                ])
            );
        } else {
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
    }
}

fn stream_frame(sequence: u64, command: Value) -> Value {
    json!({"session":"0123456789abcdef0123456789abcdef","sequence":sequence,"command":command})
}

fn stream_step(
    session: &mut openpilot_model_runtime::compiler_schedule::export::StreamSession,
    sequence: u64,
    command: Value,
) -> Result<Value, String> {
    let frame =
        serde_json::from_value(stream_frame(sequence, command)).map_err(|e| e.to_string())?;
    serde_json::to_value(session.handle(frame)?).map_err(|e| e.to_string())
}

#[test]
fn stream_checkpoints_preserve_lazy_errors_and_latch_without_final_success() {
    use openpilot_model_runtime::compiler_schedule::export::StreamSession;
    let mut session = StreamSession::default();
    let reply = stream_step(
        &mut session,
        0,
        json!({"op":"begin","version":1,"initialized":[{"key":10,"data":"00"}]}),
    )
    .unwrap();
    assert_eq!(reply["kind"], "checkpoint");
    assert_eq!(reply["complete"], false);
    let error = stream_step(
        &mut session,
        1,
        json!({"op":"event","event":{"op":"read","buffer":storage(10,0,8)}}),
    )
    .unwrap_err();
    assert!(error.contains("initialized"));
    assert!(stream_step(
        &mut session,
        2,
        json!({"op":"finish","stages":[],"retained_calls":[],"emitted_by_source":[0]})
    )
    .unwrap_err()
    .contains("failed"));
    // The same malformed initializer is harmless when unused or fully written.
    for written in [false, true] {
        let mut session = StreamSession::default();
        stream_step(
            &mut session,
            0,
            json!({"op":"begin","version":1,"initialized":[{"key":10,"data":"00"}]}),
        )
        .unwrap();
        let mut seq = 1;
        if written {
            for op in ["write", "read"] {
                stream_step(
                    &mut session,
                    seq,
                    json!({"op":"event","event":{"op":op,"buffer":storage(10,0,8)}}),
                )
                .unwrap();
                seq += 1;
            }
        }
        let result = stream_step(
            &mut session,
            seq,
            json!({"op":"finish","stages":[],"retained_calls":[],"emitted_by_source":[0]}),
        )
        .unwrap();
        assert_eq!(result["kind"], "final");
        assert_eq!(result["complete"], true);
        assert_eq!(result["plan"]["weights"], "");
        assert!(stream_step(
            &mut session,
            seq + 1,
            json!({"op":"begin","version":1,"initialized":[]})
        )
        .is_err());
    }
}

#[test]
fn stream_native_cores_match_original_bytes_and_launch_failure_precedes_read() {
    use openpilot_model_runtime::compiler_schedule::export::StreamSession;
    let original = request();
    let mut session = StreamSession::default();
    stream_step(
        &mut session,
        0,
        json!({"op":"begin","version":1,"initialized":original["initialized"]}),
    )
    .unwrap();
    let mut seq = 1;
    for event in original["events"].as_array().unwrap() {
        let reply = stream_step(&mut session, seq, json!({"op":"event","event":event})).unwrap();
        assert_eq!(reply["complete"], false);
        seq += 1;
    }
    stream_step(
        &mut session,
        seq,
        json!({"op":"program","key":50,"program":original["programs"][0]}),
    )
    .unwrap();
    seq += 1;
    stream_step(
        &mut session,
        seq,
        json!({"op":"launch","key":50,"global":original["launches"][0]["global"],"local":null}),
    )
    .unwrap();
    let result = stream_step(
        &mut session,
        seq + 1,
        json!({"op":"finish","retained_calls":[100],"emitted_by_source":[0,1],
        "stages":[{"name":"prepare","calls":[]},{"name":"model","calls":[100]}]}),
    )
    .unwrap();
    assert_eq!(result["plan"]["weights"], "0000000005060708");
    assert_eq!(result["plan"]["event_views"], json!([0, 1, 2, 0, 0]));
    assert_eq!(
        result["plan"]["entrypoints"],
        json!([
            {"name":"prepare","start":0,"end":0},{"name":"model","start":0,"end":1}
        ])
    );
    let mut session = StreamSession::default();
    stream_step(
        &mut session,
        0,
        json!({"op":"begin","version":1,"initialized":[]}),
    )
    .unwrap();
    stream_step(
        &mut session,
        1,
        json!({"op":"program","key":50,"program":original["programs"][0]}),
    )
    .unwrap();
    let error = stream_step(&mut session, 2, json!({"op":"launch","key":50,
        "global":[{"type":"integer","value":"0"},{"type":"integer","value":"1"},{"type":"integer","value":"1"}],"local":null})).unwrap_err();
    assert!(error.contains("launch dimensions"));
    assert!(stream_step(
        &mut session,
        3,
        json!({"op":"event","event":{"op":"read","buffer":storage(10,0,8)}})
    )
    .unwrap_err()
    .contains("failed"));
}

#[test]
fn actual_stream_cli_requires_exact_owner_frames_and_eof_before_final() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let begin = stream_frame(0, json!({"op":"begin","version":1,"initialized":[]}));
    let finish = stream_frame(
        1,
        json!({"op":"finish","stages":[],"retained_calls":[],"emitted_by_source":[0]}),
    );
    for fault in [
        "none",
        "missing-finish",
        "owner",
        "sequence",
        "unknown",
        "header",
        "payload",
        "length",
        "trailing",
    ] {
        let mut frames = vec![begin.clone(), finish.clone()];
        match fault {
            "missing-finish" => {
                frames.pop();
            }
            "owner" => frames[1]["session"] = json!("11111111111111111111111111111111"),
            "sequence" => frames[1]["sequence"] = json!(99),
            "unknown" => frames[1]["command"]["unexpected"] = json!(true),
            _ => (),
        }
        let mut wire = Vec::new();
        for frame in frames {
            let raw = serde_json::to_vec(&frame).unwrap();
            wire.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            wire.extend_from_slice(&raw);
        }
        match fault {
            "header" => wire = vec![0, 0, 0],
            "payload" => wire = vec![0, 0, 0, 10, b'{', b'}'],
            "length" => wire = (256u32 * 1024 * 1024 + 1).to_be_bytes().to_vec(),
            "trailing" => wire.push(b'x'),
            _ => (),
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
            .args([
                "--library",
                "/unused-export-stream-provider",
                "--export-stream",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        for part in wire.chunks(3) {
            if input.write_all(part).is_err() {
                break;
            }
        }
        drop(input);
        let result = child.wait_with_output().unwrap();
        assert_eq!(
            result.status.success(),
            fault == "none",
            "{fault}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut bytes = result.stdout.as_slice();
        let mut replies = Vec::<Value>::new();
        while !bytes.is_empty() {
            assert!(bytes.len() >= 4);
            let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            assert!(bytes.len() >= 4 + length);
            replies.push(serde_json::from_slice(&bytes[4..4 + length]).unwrap());
            bytes = &bytes[4 + length..];
        }
        if fault == "none" {
            assert_eq!(replies.len(), 2);
            assert_eq!(replies[0]["complete"], false);
            assert_eq!(replies[1]["complete"], true);
            assert_eq!(replies[1]["plan"]["version"], 1);
            assert_eq!(replies[1]["plan"]["weights"], "");
        } else {
            assert!(
                replies
                    .iter()
                    .all(|row| row["kind"] == "checkpoint" && row["complete"] == false),
                "{fault}"
            );
        }
    }
}

fn registry_program(key: u64, source: &str, binary: &str) -> Value {
    json!({"op":"program","key":key,"program":{"source":source,"binary":binary}})
}

fn registry_launch(key: u64) -> Value {
    json!({"op":"launch","key":key,"global":[{"type":"integer","value":"64"}],
           "local":[{"type":"integer","value":"1"}]})
}

fn registry_commit(key: u64, name: &str, mut arguments: Value) -> Value {
    for group in arguments.as_array_mut().unwrap() {
        for argument in group.as_array_mut().unwrap() {
            if argument["kind"] == "image" {
                for field in ["width", "height", "pitch", "element_bytes"] {
                    argument[field] = json!(argument[field].to_string());
                }
            }
        }
    }
    json!({"op":"kernel_commit","key":key,"name":name,"arguments":arguments})
}

fn registry_arguments() -> Value {
    json!([[{"kind":"image","width":4,"height":3,"pitch":32,"element_bytes":2}],
           [{"kind":"buffer"}]])
}

fn registry_finish(count: usize) -> Value {
    json!({"op":"finish","stages":[],"retained_calls":(0..count).collect::<Vec<_>>(),
           "emitted_by_source":(0..=count).collect::<Vec<_>>()})
}

#[test]
fn registry_v2_excludes_source_and_key_and_keeps_first_program_and_repeated_calls() {
    use openpilot_model_runtime::compiler_schedule::export::StreamSession;
    let mut session = StreamSession::default();
    let commands = [
        json!({"op":"begin","version":2,"initialized":[]}),
        registry_program(90, "first source", "616263"),
        registry_launch(90),
        registry_commit(90, "kernel", registry_arguments()),
        registry_program(2, "different SOURCE, same signature", "616263"),
        registry_launch(2),
        registry_commit(2, "kernel", registry_arguments()),
        registry_launch(90),
        registry_commit(90, "kernel", registry_arguments()),
        registry_finish(3),
    ];
    let replies = commands
        .into_iter()
        .enumerate()
        .map(|(sequence, command)| stream_step(&mut session, sequence as u64, command).unwrap())
        .collect::<Vec<_>>();
    for sequence in [3, 6, 8] {
        assert_eq!(replies[sequence]["kind"], "kernel");
        assert_eq!(replies[sequence]["complete"], false);
        assert_eq!(replies[sequence]["index"], 0);
    }
    let final_plan = &replies[9]["plan"];
    // Independent original Kernel dataclass signature for b"abc". Neither a
    // candidate hash nor program-key sorting supplies the expected registry.
    assert_eq!(
        final_plan["registry"],
        json!({"kernels":[{
        "program_key":90,"name":"kernel","binary_bytes":3,
        "binary_sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "arguments":registry_arguments()}],"launch_kernels":[0,0,0]})
    );
    assert_eq!(final_plan["launches"].as_array().unwrap().len(), 3);
    assert_eq!(final_plan["version"], 1);
    assert!(plan(request()).unwrap().get("registry").is_none());
    let mut legacy = StreamSession::default();
    stream_step(
        &mut legacy,
        0,
        json!({"op":"begin","version":1,"initialized":[]}),
    )
    .unwrap();
    assert!(
        stream_step(&mut legacy, 1, registry_finish(0)).unwrap()["plan"]
            .get("registry")
            .is_none()
    );
}

#[test]
fn registry_v2_distinguishes_every_argument_field_name_binary_and_nested_position() {
    use openpilot_model_runtime::compiler_schedule::export::StreamSession;
    let mut variants = vec![
        ("kernel", "616263", registry_arguments()),
        ("renamed", "616263", registry_arguments()),
        ("kernel", "00", registry_arguments()),
    ];
    for field in ["width", "height", "pitch", "element_bytes"] {
        let mut arguments = registry_arguments();
        arguments[0][0][field] = json!(arguments[0][0][field].as_i64().unwrap() + 1);
        variants.push(("kernel", "616263", arguments));
    }
    variants.push((
        "kernel",
        "616263",
        json!([[{"kind":"buffer"}],[{"kind":"buffer"}]]),
    ));
    let mut swapped = registry_arguments().as_array().unwrap().clone();
    swapped.swap(0, 1);
    variants.push(("kernel", "616263", json!(swapped)));
    let mut grouped = registry_arguments();
    grouped[0]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"buffer"}));
    let mut inner_swapped = grouped.clone();
    inner_swapped[0].as_array_mut().unwrap().swap(0, 1);
    variants.push(("kernel", "616263", grouped));
    variants.push(("kernel", "616263", inner_swapped));
    let mut empty_group = registry_arguments();
    empty_group.as_array_mut().unwrap().push(json!([]));
    variants.push(("kernel", "616263", empty_group));
    let mut session = StreamSession::default();
    stream_step(
        &mut session,
        0,
        json!({"op":"begin","version":2,"initialized":[]}),
    )
    .unwrap();
    let mut sequence = 1;
    for (index, (name, binary, arguments)) in variants.iter().enumerate() {
        for command in [
            registry_program(index as u64, "same SOURCE", binary),
            registry_launch(index as u64),
            registry_commit(index as u64, name, arguments.clone()),
        ] {
            let reply = stream_step(&mut session, sequence, command).unwrap();
            if reply["kind"] == "kernel" {
                assert_eq!(reply["index"], index);
            }
            sequence += 1;
        }
    }
    let result = stream_step(&mut session, sequence, registry_finish(variants.len())).unwrap();
    let kernels = result["plan"]["registry"]["kernels"].as_array().unwrap();
    assert_eq!(kernels.len(), 12);
    assert_eq!(kernels[2]["binary_bytes"], 1);
    assert_eq!(
        kernels[2]["binary_sha256"],
        "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"
    );
    for (index, (name, _, arguments)) in variants.iter().enumerate() {
        assert_eq!(kernels[index]["program_key"], index);
        assert_eq!(kernels[index]["name"], *name);
        assert_eq!(kernels[index]["arguments"], *arguments);
    }
}

#[test]
fn registry_v2_rejects_cross_version_missing_duplicate_wrong_commit_and_read_failure() {
    use openpilot_model_runtime::compiler_schedule::export::StreamSession;
    for fault in [
        "v1",
        "no-launch",
        "wrong-key",
        "missing",
        "duplicate",
        "next-launch",
        "read",
    ] {
        let mut session = StreamSession::default();
        let mut sequence = 0;
        let mut commands = vec![
            json!({"op":"begin","version":if fault == "v1" {1} else {2},"initialized":[]}),
            registry_program(90, "source", "616263"),
        ];
        if fault != "no-launch" {
            commands.push(registry_launch(90));
        }
        if fault == "duplicate" {
            commands.push(registry_commit(90, "kernel", registry_arguments()));
        }
        for command in commands {
            stream_step(&mut session, sequence, command).unwrap();
            sequence += 1;
        }
        let command = match fault {
            "missing" => registry_finish(1),
            "next-launch" => registry_launch(90),
            "read" => json!({"op":"event","event":{"op":"read","buffer":storage(10,0,8)}}),
            "wrong-key" => registry_commit(2, "kernel", registry_arguments()),
            _ => registry_commit(90, "kernel", registry_arguments()),
        };
        let failure = stream_step(&mut session, sequence, command).unwrap_err();
        assert!(
            failure.contains(if fault == "read" {
                "initialized"
            } else {
                "commit"
            }),
            "{fault}: {failure}"
        );
        assert!(stream_step(&mut session, sequence, registry_finish(1))
            .unwrap_err()
            .contains("failed or finished"));
    }
    // New wire schemas remain closed; a caller cannot supply its own hash/len.
    for field in ["binary_sha256", "binary_bytes", "source"] {
        let mut commit = registry_commit(1, "kernel", registry_arguments());
        commit[field] = json!("candidate-authored");
        assert!(serde_json::from_value::<
            openpilot_model_runtime::compiler_schedule::export::StreamFrame,
        >(stream_frame(0, commit))
        .is_err());
    }
    for argument in [
        json!({"kind":"buffer","extra":0}),
        json!({"kind":"unknown"}),
        json!({"kind":"image","width":"4","height":"3","pitch":"32"}),
        json!({"kind":"image","width":true,"height":"3","pitch":"32","element_bytes":"2"}),
        json!({"kind":"image","width":"4","height":"3","pitch":"32","element_bytes":"2","extra":0}),
    ] {
        let commit = json!({"op":"kernel_commit","key":1,"name":"kernel","arguments":[[argument]]});
        assert!(serde_json::from_value::<
            openpilot_model_runtime::compiler_schedule::export::StreamFrame,
        >(stream_frame(0, commit))
        .is_err());
    }
    for version in [0, 3] {
        assert!(stream_step(
            &mut StreamSession::default(),
            0,
            json!({"op":"begin","version":version,"initialized":[]})
        )
        .is_err());
    }
}

#[test]
fn actual_registry_stream_cli_preserves_kernel_reply_and_final_eof_gate() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    for fault in ["none", "missing-commit", "trailing"] {
        let mut commands = vec![
            json!({"op":"begin","version":2,"initialized":[]}),
            registry_program(90, "source", "616263"),
            registry_launch(90),
        ];
        if fault != "missing-commit" {
            commands.push(registry_commit(90, "kernel", registry_arguments()));
        }
        commands.push(registry_finish(1));
        let mut wire = Vec::new();
        for (sequence, command) in commands.into_iter().enumerate() {
            let raw = serde_json::to_vec(&stream_frame(sequence as u64, command)).unwrap();
            wire.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            wire.extend_from_slice(&raw);
        }
        if fault == "trailing" {
            wire.push(b'x');
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_offline-qcom-compile"))
            .args(["--library", "/unused-registry-provider", "--export-stream"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(&wire);
        let result = child.wait_with_output().unwrap();
        assert_eq!(
            result.status.success(),
            fault == "none",
            "{fault}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut bytes = result.stdout.as_slice();
        let mut replies = Vec::<Value>::new();
        while !bytes.is_empty() {
            assert!(bytes.len() >= 4);
            let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            assert!(bytes.len() >= 4 + length);
            replies.push(serde_json::from_slice(&bytes[4..4 + length]).unwrap());
            bytes = &bytes[4 + length..];
        }
        if fault == "none" {
            assert!(result.stderr.is_empty());
            assert_eq!(replies[3]["kind"], "kernel");
            assert_eq!(replies[3]["index"], 0);
            assert_eq!(
                replies.last().unwrap()["plan"]["registry"]["launch_kernels"],
                json!([0])
            );
        } else {
            assert!(replies.iter().all(|reply| reply["complete"] == false));
        }
    }
}

// The actual maintained Python emitter invokes the actual existing CLI. No
// model/compiler/provider/GPU is executed: a genuine COPY UOp needs no kernel.
const CALLER: &str = r#"
import copy, hashlib, json, pathlib, struct, sys
root, binary, out = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3])
sys.path.insert(0, str(root))
sys.path.insert(0, str(root/'tinygrad_repo'))
sys.path.insert(0, str(root/'rust/tools'))
from tinygrad import Tensor, dtypes
from tinygrad.device import Device
from tinygrad.uop.ops import Ops, UOp
from model_export import Bindings
from model_export.offline_model import InitializedBuffer, OfflineModel
from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
from model_export.offline_schedule import Emission, emit_schedule
from model_export import offline_schedule, offline_buffers, buffers
def no_python_table(*args, **kwargs):
    raise AssertionError('offline emission used a Python buffer table')
offline_buffers.OfflineBufferTable.__init__ = no_python_table
buffers.BufferTable.__init__ = no_python_table
assert not hasattr(offline_schedule, 'launch_dimensions')
opened = Device._Device__get_canonicalized_item
def no_gpu(name):
    assert not name.startswith('QCOM'), 'unexpected GPU allocation'
    return opened(name)
Device._Device__get_canonicalized_item = no_gpu
source = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
target = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
model = OfflineModel(Bindings({}, {'out':target}), (InitializedBuffer(source.uop.base, b'\x00\x01\x02\x03\x04\x05\x06\x07'),))
linear = UOp(Ops.LINEAR, src=(UOp(Ops.COPY).call(target.uop.buf_uop, source.uop.buf_uop),))
compiler = OfflineQcomCompiler(CompilerCommand((binary, '--library', str(out/'unused-provider')), 30))
actual_finish = offline_schedule._NativeExportStream.finish
actual_write = offline_schedule._NativeExportStream._write_all
def fragmented_write(self, raw, deadline):
    for offset in range(0, len(raw), 3):
        actual_write(self, raw[offset:offset+3], deadline)
offline_schedule._NativeExportStream._write_all = fragmented_write
observed = {}
def record_native(self, request):
    assert request['version'] == 2 and 'stage_ends' not in request
    assert request['retained_calls'] == [id(call) for call in linear.src]
    result = actual_finish(self, request)
    observed['raw'] = copy.deepcopy(result)
    return result
offline_schedule._NativeExportStream.finish = record_native
emission = Emission(model, linear, (('empty',()),('copy',tuple(linear.src))))
result = emit_schedule(emission, compiler, out/'bundle')
graph = json.loads((out/'bundle/graph.json').read_bytes())
assert result.weights == (out/'bundle/weights.bin').read_bytes() == bytes(range(8))
assert graph['calls'] == [{'op':'copy', 'source':1, 'destination':0}]
assert graph['entrypoints'] == [{'name':'empty','start':0,'end':0},{'name':'copy','start':0,'end':1}]
assert len(result.programs) == 0 and graph['kernels'] == []
offline_schedule._NativeExportStream._write_all = actual_write
# Fixed original explicit-local pass-through regression values; not native-derived expected.
geometry = {'global':[{'type':'float','value':102.49635675101081},{'type':'integer','value':'2'},{'type':'boolean','value':True}],
            'local':[{'type':'float','value':51.248178375505404},{'type':'float','value':1.0},{'type':'integer','value':'1'}]}
wire_request = {'events':[], 'launches':[copy.deepcopy(geometry)], 'stages':[], 'emitted_by_source':[0,1]}
wire_plan = {'version':1, 'allocations':[], 'views':[], 'event_views':[], 'weights':'',
             'weights_sha256':hashlib.sha256(b'').hexdigest(), 'launches':[geometry], 'entrypoints':[]}
decoded = offline_schedule._decode_native_plan(wire_plan, wire_request)['launches'][0]
assert tuple(type(value) for value in decoded[0]) == (float,int,bool)
assert tuple(type(value) for value in decoded[1]) == (float,float,int)
assert struct.pack('>d',decoded[0][0]) == struct.pack('>d',102.49635675101081)
assert struct.pack('>d',decoded[1][0]) == struct.pack('>d',51.248178375505404)
for dimension in ({'type':'float','value':True}, {'type':'unknown','value':1}):
    broken = copy.deepcopy(wire_plan)
    broken['launches'][0]['global'][0] = dimension
    try:
        offline_schedule._decode_native_plan(broken, wire_request)
    except RuntimeError:
        pass
    else:
        raise AssertionError('invalid typed dimension accepted')
# Fault injection is rejection-only, never an oracle/positive expected result.
for fault in ('unknown', 'boolean-index', 'weights', 'launch-count', 'stage-name',
              'registry-missing', 'registry-field', 'registry-handle', 'registry-count'):
    invalid = copy.deepcopy(observed['raw'])
    if fault == 'unknown': invalid['unexpected'] = True
    elif fault == 'boolean-index': invalid['event_views'][0] = True
    elif fault == 'weights': invalid['weights'] = 'gg'
    elif fault == 'launch-count': invalid['launches'] = [{}]
    elif fault == 'stage-name': invalid['entrypoints'][0]['name'] = 'wrong'
    elif fault == 'registry-missing': del invalid['registry']
    elif fault == 'registry-field': invalid['registry']['unexpected'] = True
    elif fault == 'registry-handle':
        invalid['registry']['kernels'] = [{'program_key':1,'name':'unheld',
            'binary_sha256':'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
            'binary_bytes':0,'arguments':[]}]
    else: invalid['registry']['launch_kernels'] = [0]
    def invalid_finish(self, request):
        actual_finish(self, request)
        return invalid
    offline_schedule._NativeExportStream.finish = invalid_finish
    rejected = out / ('invalid-' + fault)
    try:
        emit_schedule(emission, compiler, rejected)
    except RuntimeError:
        pass
    else:
        raise AssertionError('malformed native response accepted: ' + fault)
    assert not rejected.exists()
offline_schedule._NativeExportStream.finish = actual_finish
# A stalled request to the real native ELF must time out, latch and reap it.
offline_schedule._NativeExportStream._write_all = lambda self, raw, deadline: None
slow = OfflineQcomCompiler(CompilerCommand((binary, '--library', str(out/'unused-provider')), 0.05))
session = offline_schedule._NativeExportStream(slow, {})
try:
    with session:
        raise AssertionError('stalled begin accepted')
except RuntimeError as error:
    assert 'timeout' in str(error), str(error)
else:
    raise AssertionError('stalled begin did not fail')
assert session.failed and session.process.returncode is not None
owned_process = session.process
try:
    session.__enter__()
except RuntimeError:
    pass
else:
    raise AssertionError('failed session spawned another process')
assert session.process is owned_process
try:
    session.request({'op':'begin','version':1,'initialized':[]})
except RuntimeError:
    pass
else:
    raise AssertionError('failed session reused')
offline_schedule._NativeExportStream._write_all = actual_write
# Invalid native invocation must not fall back to the still-present Python table.
bad = OfflineQcomCompiler(CompilerCommand((binary, '--library', str(out/'unused-provider'), '--bad'), 30))
try:
    emit_schedule(Emission(model,linear),bad,out/'must-not-exist')
except RuntimeError as error:
    assert 'native export-stream' in str(error), str(error)
else:
    raise AssertionError('native failure accepted')
assert not (out/'must-not-exist').exists()
print('MAINTAINED_OFFLINE_EXPORT_NATIVE_COPY_PASS')
"#;

#[test]
#[ignore = "GHA-only actual Python maintained emitter and native CLI boundary; no inference"]
fn maintained_python_export_calls_native_plan_without_fallback() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = tempfile::tempdir().unwrap();
    let result = std::process::Command::new("python3")
        .args(["-I", "-B", "-c", CALLER])
        .arg(root)
        .arg(env!("CARGO_BIN_EXE_offline-qcom-compile"))
        .arg(out.path())
        .env("DEV", "CPU")
        .env("CACHELEVEL", "0")
        .env("DEBUG", "0")
        .env("PROFILE", "0")
        .env("TYPED", "0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        "MAINTAINED_OFFLINE_EXPORT_NATIVE_COPY_PASS"
    );
}

// GHA-only independent expected: the pinned original emitter prefix stops
// BEFORE its first native call. It uses original Python tables/launch/stages.
// Both passes use genuine to_program/compiler bytes; no fixture compiler/fallback.
const MIXED_CALLER: &str = r#"
from __future__ import annotations
import ast, copy, dataclasses, hashlib, json, os, pathlib, re, struct, sys, types
root, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
sys.path[:0] = [str(root), str(root/'tinygrad_repo'), str(root/'rust/tools')]
from tinygrad import Context, Tensor, dtypes
from tinygrad.device import Device
from tinygrad.uop.ops import Ops, ProgramInfo, UOp
from model_export import Bindings, offline_schedule as current, offline_buffers, buffers, schema
from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
from model_export.offline_model import InitializedBuffer, OfflineModel
reference = pathlib.Path(os.environ['COMPILER_EMISSION_REFERENCE_SCHEDULE'])
assert reference.is_absolute() and reference.resolve(strict=True) == reference and reference.is_file()
assert reference.stat().st_size <= 1024**2
pins = {
    reference: 'bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144',
    root/'rust/tools/model_export/offline_buffers.py': '9ab6c8c777b04d374750779fabfa75bb2b2af64fb6f8ce3ff8edd7a14138de4f',
    root/'rust/tools/model_export/buffers.py': '9429df49f408cda89f5b862fc0d51982668234cc18cb28ca0e83fd3d1a5bc7de',
    root/'rust/tools/model_export/schema.py': '720abcc396852a395689c4d37f9487822f951166a36b5efe8586dce5f656c6a9',
}
python = pathlib.Path(os.environ['STAGE_REFERENCE_PYTHON'])
assert pathlib.Path(sys.executable) == python and python.resolve(strict=True) == python
proof = pathlib.Path(os.environ['CI_JOB_TEMP'])/'proof'
python_hash = (proof/'stage-oracle-python.sha256').read_bytes()
python_version = (proof/'stage-oracle-python.version').read_bytes()
assert len(python_hash) <= 4096 and len(python_version) <= 4096 and python.stat().st_size <= 64*1024**2
python_sha = hashlib.sha256(python.read_bytes()).hexdigest()
assert python_hash == f'{python_sha}  {python}\n'.encode()
assert python_version == f'Python {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}\n'.encode()
pins[python] = python_sha
for path, digest in pins.items():
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, str(path)
def identity(path):
    assert path.is_file() and not path.is_symlink()
    value = path.stat()
    return [value.st_dev,value.st_ino,value.st_mode,value.st_uid,value.st_gid,value.st_nlink,
            value.st_size,value.st_mtime_ns,value.st_ctime_ns]
witnesses = {str(path):identity(path) for path in (*pins, proof/'stage-oracle-python.sha256', proof/'stage-oracle-python.version')}
evidence = pathlib.Path(os.environ['COMPILER_EMISSION_EVIDENCE'])
assert evidence.is_absolute() and evidence.parent.resolve(strict=True) == evidence.parent
assert evidence.parent.is_dir() and evidence.parent.stat().st_uid == os.getuid()
evidence.mkdir(mode=0o700)
written = 0
def record(name, raw):
    global written
    assert len(raw) <= 1024**2 and written + len(raw) <= 8*1024**2
    fd = os.open(evidence/name, os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)
    written += len(raw)
record('original-offline_schedule.py', reference.read_bytes())
record('python.sha256', python_hash)
record('python.version', python_version)
tree = ast.parse(reference.read_bytes(), filename=str(reference))
launch = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'launch_dimensions')
emit = copy.deepcopy(next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'emit_schedule'))
cut = next(i for i, node in enumerate(emit.body) if isinstance(node, ast.Assign)
           and any(isinstance(target, ast.Name) and target.id == 'native' for target in node.targets))
prefix = emit.body[:cut]
assert not any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == '_native_export_plan'
               for statement in prefix for node in ast.walk(statement))
emit.body = prefix + [ast.Return(value=ast.Tuple(elts=[ast.Name(id=name, ctx=ast.Load())
    for name in ('manifest', 'weights', 'programs', 'serialized')], ctx=ast.Load()))]
original = dict(vars(current), re=re, struct=struct, ProgramInfo=ProgramInfo,
                OfflineBufferTable=offline_buffers.OfflineBufferTable, entrypoint_version=schema.entrypoint_version)
exec(compile(ast.fix_missing_locations(ast.Module(body=[launch, emit], type_ignores=[])), str(reference), 'exec'), original)
opened = Device._Device__get_canonicalized_item
def no_gpu(name):
    assert not name.startswith('QCOM'), 'unexpected GPU allocation'
    return opened(name)
Device._Device__get_canonicalized_item = no_gpu
source = Tensor.empty(16, dtype=dtypes.float32, device='QCOM')
copied = Tensor.empty(16, dtype=dtypes.float32, device='QCOM')
output = (copied + 1).contiguous()
data = struct.pack('<16f', *range(16))
model = OfflineModel(Bindings({'input':source}, {'output':output}), (InitializedBuffer(source.uop.base, data),))
command = json.loads(os.environ['QCOM_COMPILER_COMMAND'])
assert type(command) is list and command and all(type(arg) is str and arg for arg in command)
compiler = OfflineQcomCompiler(CompilerCommand(tuple(command), 60))
kernel_calls = tuple(current.schedule_outputs((output,)).src)
assert kernel_calls and any(call.src[0].op is Ops.SINK for call in kernel_calls)
copy_call = UOp(Ops.COPY).call(copied.uop.buf_uop, source.uop.buf_uop)
linear = UOp(Ops.LINEAR, src=(copy_call, *kernel_calls, *kernel_calls))
stages = (('copy',(copy_call,)), ('compute',kernel_calls), ('reuse',kernel_calls), ('empty',()))
ends = (('copy',1), ('compute',1+len(kernel_calls)), ('reuse',len(linear.src)), ('empty',len(linear.src)))
with Context(NO_MEMORY_PLANNER=0):
    expected, weights, programs, serialized = original['emit_schedule'](
        types.SimpleNamespace(model=model, linear=linear, stage_ends=ends), compiler, out/'unused-original-destination')
graph_bytes = (json.dumps(serialized, indent=2) + '\n').encode()
record('original-graph.json', graph_bytes)
record('original-weights.bin', weights)
record('original-input.json', json.dumps({'initializer_hex':data.hex(), 'stage_ends':ends,
    'retained_call_ids':[id(call) for call in linear.src], 'stage_call_ids':[(name,[id(call) for call in calls]) for name,calls in stages]}).encode())
for index, program in enumerate(programs):
    record(f'original-kernel-{index}.cl', program.src[3].arg.encode())
    record(f'original-kernel-{index}.bin', program.src[4].arg)
record('original-program-ir.json', json.dumps([{'index':index,'program_id':id(program),
    'ir_key':program.src[2].key.hex()} for index,program in enumerate(programs)]).encode())
original_table_init = offline_buffers.OfflineBufferTable.__init__
original_base_init = buffers.BufferTable.__init__
original_version = schema.entrypoint_version
def no_python_table(*args, **kwargs):
    raise AssertionError('candidate invoked original Python bookkeeping')
offline_buffers.OfflineBufferTable.__init__ = no_python_table
buffers.BufferTable.__init__ = no_python_table
schema.entrypoint_version = no_python_table
original_kernel_hash = current.Kernel.__hash__
current.Kernel.__hash__ = no_python_table
# Only the weight integrity reader may still use Python SHA. Binary signature
# hashing must be performed by the native registry, after the original pass.
original_hashlib = current.hashlib
def no_python_binary_hash(raw):
    assert all(raw != program.src[4].arg for program in programs), 'Python binary signature hash'
    return original_hashlib.sha256(raw)
current.hashlib = types.SimpleNamespace(sha256=no_python_binary_hash)
for probe in (lambda:hash(expected.kernels[0]), lambda:current.hashlib.sha256(programs[0].src[4].arg)):
    try:
        probe()
    except AssertionError:
        pass
    else:
        raise AssertionError('Python registry poison did not arm')
assert not hasattr(current, 'launch_dimensions')
native = current._NativeExportStream.finish
native_kernel = current._NativeExportStream.kernel
commits = []
def observe_kernel(self, program, name, arguments):
    index = native_kernel(self, program, name, arguments)
    commits.append((index,program))
    return index
current._NativeExportStream.kernel = observe_kernel
def observe_native(self, request):
    assert request['version'] == 2 and 'stage_ends' not in request
    assert request['retained_calls'] == [id(call) for call in linear.src]
    assert request['stages'] == [{'name':name,'calls':[id(call) for call in calls]} for name,calls in stages]
    record('candidate-request.json', json.dumps(request, separators=(',', ':'), allow_nan=False).encode())
    result = native(self, request)
    record('candidate-plan.json', json.dumps(result, separators=(',', ':'), allow_nan=False).encode())
    return result
current._NativeExportStream.finish = observe_native
planner = current.plan_linear
def observe_planner(linear, held):
    planned = planner(linear, held)
    assert len(planned.src) == len(linear.src) and any(a is not b for a,b in zip(linear.src,planned.src,strict=True))
    return planned
current.plan_linear = observe_planner
with Context(NO_MEMORY_PLANNER=0):
    result = current.emit_schedule(current.Emission(model,linear,stages), compiler, out/'candidate')
assert (out/'candidate/graph.json').read_bytes() == graph_bytes
assert result.weights == (out/'candidate/weights.bin').read_bytes() == weights
assert len(result.programs) == len(programs) and len(result.manifest.calls) == len(linear.src)
for index, program in enumerate(programs):
    assert (out/f'candidate/kernel-{index}.cl').read_bytes() == program.src[3].arg.encode()
    assert (out/f'candidate/kernel-{index}.bin').read_bytes() == program.src[4].arg
    assert result.programs[index].src[2].key == program.src[2].key
    assert result.programs[index] is next(actual for kernel,actual in commits if kernel == index)
record('candidate-program-ir.json', json.dumps([{'index':index,'program_id':id(program),
    'ir_key':program.src[2].key.hex()} for index,program in enumerate(result.programs)]).encode())
current._NativeExportStream.finish = native
current._NativeExportStream.kernel = native_kernel
current.Kernel.__hash__ = original_kernel_hash
current.hashlib = original_hashlib
current.plan_linear = planner
# Independent negative-order observations use the same pinned original prefix.
# Clear only the test cache and count BOTH lowering entry and genuine provider calls.
from tinygrad.codegen import to_program_cache
real_program, real_compile = current.to_program, compiler.compile
missing = Tensor.empty(16, dtype=dtypes.float32, device='QCOM')
first = (missing + 1).contiguous()
future = (missing + 2).contiguous()
first_calls = tuple(current.schedule_outputs((first,)).src)
future_calls = tuple(current.schedule_outputs((future,)).src)
assert len(first_calls) == len(future_calls) == 1
two = UOp(Ops.LINEAR, src=(*first_calls,*future_calls))
two_stages = (('first',first_calls),('future',future_calls))
two_ends = (('first',1),('future',2))
bad_input = OfflineModel(Bindings({'input':missing},{'first':first,'future':future}), (InitializedBuffer(missing.uop.base,b'\x00'),))
bad_read = OfflineModel(Bindings({}, {'first':first,'future':future}), ())
copy_source = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
copy_target = Tensor.empty(8, dtype=dtypes.uint8, device='QCOM')
leading = UOp(Ops.COPY).call(copy_target.uop.buf_uop,copy_source.uop.buf_uop)
leading_then_kernels = UOp(Ops.LINEAR, src=(leading,*first_calls,*future_calls))
leading_stages = (('copy',(leading,)),*two_stages)
leading_ends = (('copy',1),('first',2),('future',3))
unused = Tensor.empty(4, dtype=dtypes.uint8, device='QCOM')
copy_only = UOp(Ops.LINEAR, src=(leading,))
copy_stages, copy_ends = (('copy',(leading,)),), (('copy',1),)
cases = [
    ('input-before-compile',bad_input,two,two_stages,two_ends,False,'initialized',0),
    ('copy-before-compile',bad_read,leading_then_kernels,leading_stages,leading_ends,False,'initialized',0),
    ('current-read-only',bad_read,two,two_stages,two_ends,False,'initialized',1),
    ('launch-before-read',bad_read,two,two_stages,two_ends,True,'launch dimensions',1),
    ('output-read-before-stage',OfflineModel(Bindings({}, {'out':copy_target}),()),UOp(Ops.LINEAR,src=()),(('',()),),(('',0),),False,'initialized',0),
]
for name, item in [('unused',unused),('write-covered',copy_target)]:
    lazy = OfflineModel(Bindings({}, {'out':copy_target}),
        (InitializedBuffer(copy_source.uop.base,bytes(range(8))),InitializedBuffer(item.uop.base,b'\xff')))
    cases.append((name,lazy,copy_only,copy_stages,copy_ends,False,None,0))
orders = []
for name, model_case, linear_case, stage_case, end_case, bad_launch, error_kind, expected_compiles in cases:
    observations = []
    expected_graph = expected_weights = None
    for candidate in (False,True):
        offline_buffers.OfflineBufferTable.__init__ = no_python_table if candidate else original_table_init
        buffers.BufferTable.__init__ = no_python_table if candidate else original_base_init
        schema.entrypoint_version = no_python_table if candidate else original_version
        current.Kernel.__hash__ = no_python_table if candidate else original_kernel_hash
        counts = {'lowering':0,'compiler':0}
        def measured_compile(source):
            counts['compiler'] += 1
            return real_compile(source)
        def measured_program(ast, renderer):
            counts['lowering'] += 1
            program = real_program(ast, renderer)
            if bad_launch:
                # Negative metadata fault AFTER genuine compilation; binary unchanged.
                return program.replace(arg=dataclasses.replace(program.arg,global_size=(0,1,1),local_size=None))
            return program
        compiler.compile = measured_compile
        current.to_program = original['to_program'] = measured_program
        to_program_cache.clear()
        destination = out/(name+('-native' if candidate else '-original'))
        failure = None
        try:
            with Context(NO_MEMORY_PLANNER=0):
                if candidate:
                    result_case = current.emit_schedule(current.Emission(model_case,linear_case,stage_case),compiler,destination)
                else:
                    result_case = original['emit_schedule'](types.SimpleNamespace(model=model_case,linear=linear_case,stage_ends=end_case),compiler,destination)
        except RuntimeError as error:
            failure = str(error)
        if error_kind is not None:
            assert failure is not None and error_kind in failure, (name,candidate,failure)
            assert not destination.exists()
        else:
            assert failure is None, (name,candidate,failure)
            if candidate:
                assert (destination/'graph.json').read_bytes() == expected_graph
                assert result_case.weights == (destination/'weights.bin').read_bytes() == expected_weights
            else:
                expected_graph = (json.dumps(result_case[3],indent=2)+'\n').encode()
                expected_weights = result_case[1]
        assert counts == {'lowering':expected_compiles,'compiler':expected_compiles}, (name,candidate,counts)
        observations.append({'candidate':candidate,'error':failure,'counts':dict(counts),'destination_exists':destination.exists()})
    orders.append({'case':name,'original_then_candidate':observations,'expected_from_original':True})
current.to_program, compiler.compile = real_program, real_compile
current.Kernel.__hash__ = original_kernel_hash
record('error-order.json', json.dumps(orders,indent=2).encode())
for path, digest in pins.items():
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, str(path)
assert (proof/'stage-oracle-python.sha256').read_bytes() == python_hash
assert (proof/'stage-oracle-python.version').read_bytes() == python_version
assert {path:identity(pathlib.Path(path)) for path in witnesses} == witnesses
record('comparison.json', json.dumps({'passed':True,'independent_python_prefix':True,'original_native_calls':0,
    'github_sha':os.environ['GITHUB_SHA'],'reference_sha256':pins[reference], 'source_pins':{str(path):digest for path,digest in pins.items()},
    'witness_identities_before_and_after':witnesses,
    'calls':len(linear.src),'kernels':len(programs),'memory_planner_rewrite_required':True,
    'python_kernel_hash_poisoned':True,'python_binary_sha_poisoned':True,'first_program_ir_compared':True,
    'ordered_failure_and_lazy_cases':len(orders),'future_compiler_after_failure':False,
    'whole_model_accepted':False,'retirement_credit':0}, indent=2).encode())
print('MAINTAINED_OFFLINE_MIXED_ORIGINAL_BOOKKEEPING_PASS')
"#;

#[test]
#[ignore = "GHA-only genuine QCOM compiler command plus pinned original Python emitter prefix; no inference"]
fn maintained_python_mixed_emission_matches_original_bookkeeping() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = tempfile::tempdir().unwrap();
    let python = std::path::PathBuf::from(
        std::env::var_os("STAGE_REFERENCE_PYTHON")
            .expect("admitted canonical STAGE_REFERENCE_PYTHON required"),
    );
    assert!(python.is_absolute() && python.is_file());
    assert_eq!(python.canonicalize().unwrap(), python);
    let output = std::process::Command::new(&python)
        .args(["-I", "-B", "-c", MIXED_CALLER])
        .arg(root)
        .arg(out.path())
        .env("DEV", "CPU")
        .env("CACHELEVEL", "0")
        .env("DEBUG", "0")
        .env("PROFILE", "0")
        .env("TYPED", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "MAINTAINED_OFFLINE_MIXED_ORIGINAL_BOOKKEEPING_PASS"
    );
}
