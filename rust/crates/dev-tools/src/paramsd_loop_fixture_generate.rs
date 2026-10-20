//! Exact input construction for the original 39-case paramsd loop controller.
//! Fresh original Python output remains the independent wire-byte oracle.
use crate::paramsd_fixture_generate::{
    calibration, car_params, car_state_valid, gps_valid, pose, Pose,
};
use capnp::serialize::write_message_to_words;
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LOOP_SHA: &str = "767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089";
const FIXTURE_SHA: &str = "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521";

fn pin(root: &Path, path: &str, expected: &str) -> Result<()> {
    if format!("{:x}", Sha256::digest(fs::read(root.join(path))?)) != expected {
        return Err(format!("paramsd original input source changed: {path}").into());
    }
    Ok(())
}

fn source_pins(root: &Path) -> Result<()> {
    pin(root, "rust/tools/paramsd_loop_fixture.py", LOOP_SHA)?;
    pin(root, "rust/tools/paramsd_fixture.py", FIXTURE_SHA)
}

fn legacy_json(field: Option<&str>) -> String {
    let ratio = "17.0";
    let stiffness = if field == Some("infinite-stiffness") {
        "Infinity"
    } else {
        "0.7"
    };
    let offset = if field == Some("negative-infinite-offset") {
        "-Infinity"
    } else {
        "2.5"
    };
    let ratio = match field {
        Some("quoted-ratio") => "\"17\"",
        Some("boolean-ratio") => "true",
        Some("nan-ratio") => "NaN",
        Some("large-ratio") => "1e+300",
        _ => ratio,
    };
    format!(
        "{{\"steerRatio\": {ratio}, \"stiffnessFactor\": {stiffness}, \"angleOffsetAverageDeg\": {offset}}}"
    )
}

fn encoded_json(text: &str, encoding: &str) -> Vec<u8> {
    match encoding {
        "utf-8-sig" => [vec![0xef, 0xbb, 0xbf], text.as_bytes().to_vec()].concat(),
        "utf-16" | "utf-16-le" | "utf-16-be" => {
            let little = encoding != "utf-16-be";
            let mut bytes = if encoding == "utf-16" {
                vec![0xff, 0xfe]
            } else {
                Vec::new()
            };
            for value in text.encode_utf16() {
                bytes.extend(if little {
                    value.to_le_bytes()
                } else {
                    value.to_be_bytes()
                });
            }
            bytes
        }
        "utf-32" | "utf-32-le" | "utf-32-be" => {
            let little = encoding != "utf-32-be";
            let mut bytes = if encoding == "utf-32" {
                vec![0xff, 0xfe, 0, 0]
            } else {
                Vec::new()
            };
            for character in text.chars() {
                let value = u32::from(character);
                bytes.extend(if little {
                    value.to_le_bytes()
                } else {
                    value.to_be_bytes()
                });
            }
            bytes
        }
        _ => unreachable!("bounded original encoding roster"),
    }
}

fn cache_packet(ratio: f64, std: &[f64]) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(20_000_000_000);
    root.set_valid(true);
    let mut parameters = root.init_live_parameters();
    parameters.set_steer_ratio(ratio as f32);
    parameters.set_stiffness_factor(0.6);
    parameters.set_angle_offset_average_deg(1.5);
    parameters.set_valid(false);
    let debug = parameters.init_debug_filter_state();
    let mut values = debug.init_std(std.len() as u32);
    for (index, &value) in std.iter().enumerate() {
        values.set(index as u32, value);
    }
    write_message_to_words(&message)
}

fn case(
    name: String,
    seed: Value,
    replay: bool,
    debug: bool,
    gps: &str,
    frames: Vec<Value>,
) -> Value {
    json!({"name":name,"car":car_params(0,None),"seed":seed,"replay":replay,
        "debug":debug,"gps":gps,"simulation":false,"frames":frames})
}

pub fn cases() -> Result<Value> {
    let old = legacy_json(None);
    let mut variants: Vec<Option<Vec<u8>>> = vec![
        None,
        Some(Vec::new()),
        Some(b"null".to_vec()),
        Some(b"{".to_vec()),
        Some(b"[]".to_vec()),
        Some(b"42".to_vec()),
        Some(b"{}".to_vec()),
        Some(vec![0xff]),
        Some(old.as_bytes().to_vec()),
    ];
    for field in [
        "quoted-ratio",
        "boolean-ratio",
        "nan-ratio",
        "infinite-stiffness",
        "negative-infinite-offset",
        "large-ratio",
    ] {
        variants.push(Some(legacy_json(Some(field)).into_bytes()));
    }
    for encoding in [
        "utf-8-sig",
        "utf-16",
        "utf-16-le",
        "utf-16-be",
        "utf-32",
        "utf-32-le",
        "utf-32-be",
    ] {
        variants.push(Some(encoded_json(&old, encoding)));
    }
    let mut result = Vec::with_capacity(39);
    for (index, value) in variants.into_iter().enumerate() {
        let mut seed = json!({"CarParamsPrevRoute":car_params(0,None)});
        if let Some(value) = value {
            seed["LiveParameters"] = json!(value);
        }
        result.push(case(
            format!("migration-{index}"),
            seed,
            false,
            true,
            "gpsLocationExternal",
            Vec::new(),
        ));
    }
    type CacheVariant<'a> = (f64, Option<usize>, &'a [f64], bool, bool);
    let cache_variants: [CacheVariant<'_>; 12] = [
        (
            15.,
            Some(0),
            &[
                0.1,
                0.2,
                0.30000000000000004,
                0.4,
                0.5,
                0.6000000000000001,
                0.7000000000000001,
                0.8,
                0.9,
            ],
            true,
            true,
        ),
        (15., Some(0), &[0.3; 9], false, true),
        (15., Some(0), &[0.3; 9], true, false),
        (7.5, Some(0), &[], true, true),
        (30., Some(0), &[], true, true),
        (7.49, Some(0), &[], true, true),
        (30.01, Some(0), &[], true, true),
        (f64::NAN, Some(0), &[], true, true),
        (15., Some(1), &[], true, true),
        (15., Some(2), &[], true, true),
        (15., None, &[], true, true),
        (15., Some(0), &[0.1, 0.2], true, true),
    ];
    for (index, (ratio, previous, std, replay, debug)) in cache_variants.into_iter().enumerate() {
        let mut seed = json!({"LiveParametersV2":cache_packet(ratio,std)});
        if let Some(previous) = previous {
            seed["CarParamsPrevRoute"] = if previous == 2 {
                json!([1, 2])
            } else {
                json!(car_params(previous, None))
            };
        }
        result.push(case(
            format!("cache-{index}"),
            seed,
            replay,
            debug,
            "gpsLocationExternal",
            Vec::new(),
        ));
    }
    result.push(case(
        "malformed-v2".into(),
        json!({"LiveParametersV2":[1,2],
        "CarParamsPrevRoute":car_params(0,None)}),
        false,
        true,
        "gpsLocationExternal",
        Vec::new(),
    ));
    result.push(case(
        "wrong-union".into(),
        json!({"LiveParametersV2":pose(2.,Pose::default()),
        "CarParamsPrevRoute":car_params(0,None)}),
        false,
        true,
        "gpsLocationExternal",
        Vec::new(),
    ));
    result.push(case(
        "v2-suppresses-migration".into(),
        json!({"LiveParameters":[123],
        "LiveParametersV2":[1,2]}),
        false,
        true,
        "gpsLocationExternal",
        Vec::new(),
    ));
    for service in ["gpsLocation", "gpsLocationExternal"] {
        let mut frames = Vec::with_capacity(1210);
        for i in 0..1210 {
            let time = 100. + i as f64 * 0.05;
            let mut messages = vec![car_state_valid(
                time,
                if i < 12 { 0. } else { 18. },
                2.,
                i != 50,
            )];
            if i % 10 == 0 {
                messages.push(calibration(time, [0.01, 0.02, -0.01]));
            }
            if i != 30 && i != 31 {
                messages.push(pose(
                    time,
                    Pose {
                        valid: i != 70,
                        ..Pose::default()
                    },
                ));
            }
            if i % 11 == 0 {
                messages.push(gps_valid(service, time, i != 22, false)?);
            }
            if (80..90).contains(&i) {
                messages = if i % 2 == 1 {
                    vec![messages.pop().expect("nonempty frame")]
                } else {
                    Vec::new()
                };
            }
            messages.reverse();
            frames.push(json!({"time":time,"messages":messages}));
        }
        result.push(case(
            format!("loop-{service}"),
            json!({}),
            false,
            true,
            service,
            frames,
        ));
    }
    Ok(json!({"cases":result}))
}

pub fn generate(root: &Path, output: &Path) -> Result<()> {
    source_pins(root)?;
    let bytes = serde_json::to_vec(&cases()?)?;
    fs::create_dir(output)?;
    fs::write(output.join("input.json"), bytes)?;
    source_pins(root)
}
