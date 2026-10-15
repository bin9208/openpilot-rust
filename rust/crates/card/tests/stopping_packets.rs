use openpilot_card::brands::hyundai::{
    canfd_acc::{self, AccInput, SccState},
    stopping::CanfdStopping,
    wire::{CanWriter, Values},
};
use serde::Deserialize;
#[derive(Deserialize)]
struct State {
    original: Option<Values>,
    wheels: [f64; 4],
    v_ego: f64,
    v_ego_raw: f64,
    a_ego: f64,
    brake: bool,
    gas: bool,
    brake_hold: bool,
    parking_brake: bool,
    can_valid: bool,
    drive: bool,
    available: bool,
    standstill: bool,
    paddle: u8,
    soft_hold: bool,
    scc_hold: bool,
}
impl State {
    fn view(&self) -> SccState<'_> {
        SccState {
            original: self.original.as_ref(),
            wheels: self.wheels,
            v_ego: self.v_ego,
            v_ego_raw: self.v_ego_raw,
            a_ego: self.a_ego,
            brake: self.brake,
            gas: self.gas,
            brake_hold: self.brake_hold,
            parking_brake: self.parking_brake,
            can_valid: self.can_valid,
            drive: self.drive,
            available: self.available,
            standstill: self.standstill,
            paddle: self.paddle,
            soft_hold: self.soft_hold,
            scc_hold: self.scc_hold,
        }
    }
}
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
fn all_pinned_stopping_packet_builders_match_9353_payloads_values_histories_and_warnings() {
    let data = zstd::stream::decode_all(
        include_bytes!("fixtures/stopping-packets-final.json.zst").as_slice(),
    )
    .unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let dbc = std::sync::Arc::new(
        openpilot_can::dbc::Dbc::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../opendbc_repo/opendbc/dbc/hyundai_canfd_generated.dbc"),
        )
        .unwrap(),
    );
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let mut controller: Option<CanfdStopping> =
            serde_json::from_value(clean(&row["before"])).unwrap();
        if let Some(controller) = &mut controller {
            controller.last_scc_jerk_upper = number(&row["before"]["last_scc_jerk_upper"]);
        }
        let mut state: State = serde_json::from_value(clean(&row["state"])).unwrap();
        state.v_ego = number(&row["state"]["v_ego"]);
        state.v_ego_raw = number(&row["state"]["v_ego_raw"]);
        state.a_ego = number(&row["state"]["a_ego"]);
        for (index, value) in state.wheels.iter_mut().enumerate() {
            *value = number(&row["state"]["wheels"][index]);
        }
        let mut input: AccInput = serde_json::from_value(clean(&row["input"])).unwrap();
        input.accel_last = number(&row["input"]["accel_last"]);
        input.value_last = number(&row["input"]["value_last"]);
        input.accel = number(&row["input"]["accel"]);
        input.jerk_u = number(&row["input"]["jerk_u"]);
        input.jerk_l = number(&row["input"]["jerk_l"]);
        let mut writer = CanWriter::from_dbc(std::sync::Arc::clone(&dbc));
        for (address, value) in row["counters"].as_object().unwrap() {
            writer
                .packer
                .counters
                .insert(address.parse().unwrap(), value.as_i64().unwrap().into());
        }
        let (frame, value) = if row["camera"].as_bool().unwrap() {
            canfd_acc::camera_acc(&mut writer, &state.view(), controller.as_mut(), &input).unwrap()
        } else {
            let (frame, value) =
                canfd_acc::acc(&mut writer, &state.view(), controller.as_mut(), &input).unwrap();
            (Some(frame), value)
        };
        assert_eq!(serde_json::json!(frame), row["result"], "frame/{i}");
        compare(
            &serde_json::json!(value),
            &row["value"],
            &format!("value/{i}"),
        );
        compare(
            &serde_json::to_value(&controller).unwrap(),
            &row["after"],
            &format!("controller/{i}"),
        );
        let warnings = controller
            .as_ref()
            .map_or(&[][..], |c| c.warnings.as_slice());
        assert_eq!(serde_json::json!(warnings), row["warnings"], "warnings/{i}");
        let counters: std::collections::BTreeMap<_, _> = writer
            .packer
            .counters
            .iter()
            .map(|(a, v)| (a.to_string(), v.to_string().parse::<i64>().unwrap()))
            .collect();
        assert_eq!(
            serde_json::json!(counters),
            row["counters_after"],
            "counters/{i}"
        );
    }
    assert_eq!(fixture["rows"].as_array().unwrap().len(), 9353);
}
