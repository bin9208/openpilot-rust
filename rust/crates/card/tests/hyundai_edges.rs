use num_traits::ToPrimitive;
use openpilot_card::brands::hyundai::{
    local_time::{country_zone, timestamp, Zone},
    settings_float, Error,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
struct Cases {
    invalid_torque: Vec<InvalidTorque>,
    sums: Vec<Sum>,
    times: Vec<Time>,
    floats: Vec<Float>,
}
#[derive(Deserialize)]
struct InvalidTorque {
    torque: String,
    error: String,
}

#[test]
fn torque_when_nonfinite_request_is_rejected_before_limiting() {
    use openpilot_card::brands::hyundai::{ApplyInput, Hyundai, Setup};
    use openpilot_cereal::car_capnp::car_control;
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let dbc = PathBuf::from(std::env::var("HYUNDAI_DBC_DIR").unwrap());
    let cases: Cases =
        serde_json::from_slice(&std::fs::read(root.join("edges.json")).unwrap()).unwrap();
    let params =
        openpilot_params::Params::open(&root.join("invalid-torque-settings"), "p").unwrap();
    let bytes = std::fs::read(root.join("source-state-0/params.bin")).unwrap();
    let mut vehicle = Hyundai::new(Setup {
        params_bytes: &bytes,
        dbc_root: &dbc,
        settings: params,
        fingerprints: &[],
        now_ns: 2_000_000_000,
    })
    .unwrap();
    let mut outputs = Vec::new();
    for case in cases.invalid_torque {
        let mut control = capnp::message::Builder::new_default();
        let mut cc = control.init_root::<car_control::Builder>();
        cc.reborrow()
            .init_actuators()
            .set_torque(case.torque.parse().unwrap());
        let result = vehicle.apply(ApplyInput {
            control: cc.into_reader(),
            now_ns: 2_000_000_000,
            model: None,
            radar: None,
        });
        assert!(
            matches!(result, Err(Error::Numeric)),
            "source {} for {}",
            case.error,
            case.torque
        );
        outputs.push((case.torque, case.error, "Numeric"));
    }
    std::fs::write(
        root.join("native-invalid-torque.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}
#[derive(Deserialize)]
struct Sum {
    values: Vec<f64>,
    total: f64,
}

#[test]
fn moving_average_when_source_compensates_float_sum() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Cases =
        serde_json::from_slice(&std::fs::read(root.join("edges.json")).unwrap()).unwrap();
    let mut outputs = Vec::new();
    for case in cases.sums {
        let value = openpilot_card::brands::hyundai::jerk::source_float_sum(&case.values);
        assert_eq!(value.to_bits(), case.total.to_bits(), "{:?}", case.values);
        outputs.push(value);
    }
    std::fs::write(
        root.join("native-sums.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}
#[derive(Deserialize)]
struct Time {
    country: i32,
    fields: BTreeMap<String, f64>,
    timestamp: Option<u64>,
}
#[derive(Deserialize)]
struct Float {
    raw: Option<String>,
    bits: u32,
    error: Option<String>,
}
#[derive(Serialize)]
struct ResultCase {
    bits: Option<u32>,
    error: Option<&'static str>,
}

#[test]
fn local_time_when_countries_dst_folds_gaps_future_and_invalid_dates() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Cases =
        serde_json::from_slice(&std::fs::read(root.join("edges.json")).unwrap()).unwrap();
    let mut outputs = Vec::new();
    for case in &cases.times {
        let zone = Zone::load(country_zone(case.country)).unwrap();
        let result = timestamp(&zone, &case.fields).unwrap();
        assert_eq!(
            result, case.timestamp,
            "country {} fields {:?}",
            case.country, case.fields
        );
        outputs.push(result);
    }
    std::fs::write(
        root.join("native-time.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}

#[test]
fn float_settings_when_prefixes_float32_rounding_invalid_and_range() {
    let root = PathBuf::from(std::env::var("HYUNDAI_FIXTURE_DIR").unwrap());
    let cases: Cases =
        serde_json::from_slice(&std::fs::read(root.join("edges.json")).unwrap()).unwrap();
    let params = openpilot_params::Params::open(&root.join("float-settings"), "p").unwrap();
    let mut outputs = Vec::new();
    for case in &cases.floats {
        match &case.raw {
            Some(value) => params.put("CarrotCruiseDecel", value.as_bytes()).unwrap(),
            None => {
                if params.get("CarrotCruiseDecel").unwrap().is_some() {
                    params.remove("CarrotCruiseDecel").unwrap();
                }
            }
        }
        let result = match settings_float::read(&params, "CarrotCruiseDecel") {
            Ok(value) => ResultCase {
                bits: Some(value.to_f32().unwrap().to_bits()),
                error: None,
            },
            Err(Error::Float(_)) => ResultCase {
                bits: None,
                error: Some("invalid"),
            },
            Err(Error::FloatRange(_)) => ResultCase {
                bits: None,
                error: Some("range"),
            },
            Err(error) => panic!("unexpected source conversion error: {error}"),
        };
        assert_eq!(result.error, case.error.as_deref(), "{:?}", case.raw);
        if result.error.is_none() {
            assert_eq!(result.bits, Some(case.bits), "{:?}", case.raw);
        }
        outputs.push(result);
    }
    std::fs::write(
        root.join("native-floats.json"),
        serde_json::to_vec(&outputs).unwrap(),
    )
    .unwrap();
}
