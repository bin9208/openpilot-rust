//! The complete rejection roster in check_control_failures.py, in source order.
use crate::{controlsd_fixture_generate as fixture, seal::Result};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::car_capnp::car_params;
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub const SOURCE_PINS: [(&str, &str); 7] = [
    (
        "rust/tools/check_control_failures.py",
        "4b8bbe608b0502f61fd1090f14872af75c9c336c26b85b9ca7bb600959e1bbea",
    ),
    (
        "rust/tools/controlsd_source.py",
        "f5a51d38c64197cc1f8786e8c10a3c5c0b54301deb8d958eae46cafcb321b7b0",
    ),
    (
        "rust/tools/controlsd_parameters.py",
        "bf9fa22d71ad808ec4697bc0ce190753c901901441a007dbe38711e7f0bc31b1",
    ),
    (
        "rust/tools/check_message_state.py",
        "928e7cf946a1fbf3c6c0bc9135c580e8f108179cfe78349956eb895f6d0c710a",
    ),
    (
        "rust/tools/locationd_loop_source.py",
        "d6239cd4e66e66d87aef577bbc49512f88aeea18270a20fe9263979153939de4",
    ),
    (
        "openpilot/selfdrive/controls/controlsd.py",
        "91b6eacee1679f49a469d15b802529efddf7dbbcb6b20e16336329a7d1487f70",
    ),
    (
        "rust/tools/controlsd_scenarios.py",
        "d83dfce64a13d033143fccbeb03bd4a270dd3837f759ad58a3bdca50fe472368",
    ),
];
pub fn source_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    fixture::verify_sources(root)?;
    let mut out = BTreeMap::new();
    for (name, expected) in SOURCE_PINS.into_iter().chain(fixture::SOURCES) {
        let actual = crate::seal::digest(&root.join(name))?;
        if actual != expected {
            return Err(format!("control rejection source changed: {name}").into());
        }
        out.insert(name.into(), actual);
    }
    Ok(out)
}
const SETTINGS: [(&str, &[u8]); 6] = [
    ("StoppingAccel", b"not a number"),
    ("SteerRatioRate", b"1e999"),
    ("CustomSR", b"bad"),
    ("LatSuspendAngleDeg", b"2147483648"),
    ("SpeedFromPCM", b"x"),
    ("DisableDM", b"x"),
];
pub fn fingerprint_values() -> Vec<Vec<u8>> {
    let mut out = vec![vec![], b"{".to_vec(), b"{1: {}}".to_vec(), vec![255]];
    for token in [
        "1_", "_1", "1__0", "0x__1", "0x1_", "0b2", "0o8", "01", "0_1", "0x", "++1",
    ] {
        out.push(format!("{{0: {{{token}: 8}}, 1: {{}}, 2: {{}}, 3: {{}}}}").into_bytes());
    }
    out
}
pub fn names() -> Vec<String> {
    SETTINGS
        .iter()
        .map(|(key, _)| format!("malformed-{key}"))
        .chain(fingerprint_values().iter().map(|bytes| {
            format!(
                "fingerprints-{}",
                bytes.iter().map(|v| format!("{v:02x}")).collect::<String>()
            )
        }))
        .chain(
            [
                "unknown-fingerprint",
                "zero-wheelbase",
                "psa-valid-tuning",
                "zero-torque-factor",
                "angle-with-torque-live-update",
                "truncated-CarParams",
            ]
            .map(str::to_owned),
        )
        .collect()
}
fn generate(input: &Value) -> Result<Value> {
    Ok(serde_json::from_str(
        &fixture::generate(&JsonValue::parse(&input.to_string())?)?.to_json()?,
    )?)
}
fn basic() -> Result<Vec<Value>> {
    let cases: Vec<Value> = serde_json::from_value(generate(&json!({"op":"fixture-cases"}))?)?;
    if cases.len() != 6 {
        return Err("basic control fixture roster changed".into());
    }
    Ok(cases)
}
fn first(case: &Value, name: &str, count: usize) -> Value {
    let mut case = case.clone();
    case["name"] = json!(name);
    case["frames"]
        .as_array_mut()
        .expect("native basic frames")
        .truncate(count);
    case
}
fn mutate_cp(case: &mut Value, name: &str) -> Result<()> {
    let bytes: Vec<u8> = serde_json::from_value(case["params"]["CarParams"].clone())?;
    let mut input = &bytes[..];
    let reader = serialize::read_message(&mut input, ReaderOptions::new())?;
    if !input.is_empty() {
        return Err("trailing CarParams fixture bytes".into());
    }
    let mut message = capnp::message::Builder::new_default();
    message.set_root(reader.get_root::<car_params::Reader<'_>>()?)?;
    let mut cp = message.get_root::<car_params::Builder<'_>>()?;
    match name {
        "unknown-fingerprint" => cp.set_car_fingerprint("MISSING"),
        "zero-wheelbase" => cp.set_wheelbase(0.),
        "psa-valid-tuning" => {
            cp.set_car_fingerprint("PSA_PEUGEOT_208");
            cp.set_brand("psa");
        }
        "zero-torque-factor" => cp
            .reborrow()
            .get_lateral_tuning()
            .get_torque()?
            .set_lat_accel_factor(0.),
        "angle-with-torque-live-update" => {
            cp.set_steer_control_type(car_params::SteerControlType::Angle)
        }
        _ => return Err("unknown rejection CarParams mutation".into()),
    }
    case["params"]["CarParams"] = json!(serialize::write_message_to_words(&message));
    Ok(())
}
pub fn health() -> Result<Value> {
    Ok(first(&basic()?[0], "health-HONDA_CIVIC-pid", 5))
}
pub fn cases() -> Result<Vec<Value>> {
    let basic = basic()?;
    let mut out = Vec::new();
    for (key, value) in SETTINGS {
        let mut case = first(&basic[0], &format!("malformed-{key}"), 5);
        case["params"][key] = json!(value);
        out.push(case);
    }
    for value in fingerprint_values() {
        let name = format!(
            "fingerprints-{}",
            value.iter().map(|v| format!("{v:02x}")).collect::<String>()
        );
        let mut case = first(&basic[3], &name, 1);
        case["params"]["FingerPrints"] = json!(value);
        out.push(case);
    }
    for (name, index) in [
        ("unknown-fingerprint", 0),
        ("zero-wheelbase", 0),
        ("psa-valid-tuning", 0),
        ("zero-torque-factor", 1),
        ("angle-with-torque-live-update", 1),
    ] {
        let mut case = first(&basic[index], name, 5);
        mutate_cp(&mut case, name)?;
        if name == "zero-torque-factor" {
            let mut frames = Vec::new();
            for row in case["frames"].as_array().ok_or("frames missing")? {
                frames.push(generate(&json!({"op":"change","row":row,"updates":{"liveTorqueParameters":{"useParams":false}}}))?);
            }
            case["frames"] = json!(frames);
        }
        if name == "angle-with-torque-live-update" {
            case["simulation"] = json!(true);
        }
        out.push(case);
    }
    let mut truncated = first(&basic[0], "truncated-CarParams", 0);
    truncated["params"]["CarParams"] = json!([0, 1, 2]);
    out.push(truncated);
    if out
        .iter()
        .map(|v| v["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        != names()
    {
        return Err("rejection roster changed".into());
    }
    Ok(out)
}
/// Shared full-schema / exact-float projection. Do not parse the deliberately
/// truncated CP as valid wire; preserve exactly those three source bytes.
pub fn projection(case: &Value) -> Result<Value> {
    let mut input = case.clone();
    let truncated = input["name"] == "truncated-CarParams";
    if truncated {
        if input["params"]["CarParams"] != json!([0, 1, 2]) {
            return Err("truncated CP mutation changed".into());
        }
        input["params"]
            .as_object_mut()
            .ok_or("params missing")?
            .remove("CarParams");
    }
    let mut out = fixture::canonical_inputs(&JsonValue::parse(&input.to_string())?)?;
    if truncated {
        out["params"]["CarParams"] = json!({"$raw-invalid-CarParams":[0,1,2]});
    }
    Ok(out)
}
