//! Reuse native vehicle parameter APIs; preserve the fixture's own tuning edits.
use super::{entries, json, merge, object, wire, Result, J};
use capnp::{message::ReaderOptions, serialize};
use openpilot_card::{
    identification::{FingerprintSource, Identification},
    vehicle_params,
};
use openpilot_cereal::car_capnp::car_params;

pub(super) const SETTINGS: &str = r#"{"FingerPrints":"{0: {}, 1: {}, 2: {}, 3: {}, 4: {}, 5: {}, 6: {}, 7: {}}","SteerRatioRate":"100","CustomSR":"0","AlwaysLateral":"1","LatSuspendAngleDeg":"45","UseLaneLineCurveSpeed":"0","LatSmoothSec":"15","SteerActuatorDelay":"20","StoppingAccel":"-50","LongTuningKpV":"100","LongTuningKiV":"100","LongTuningKf":"100","DisableDM":"0","SpeedFromPCM":"0","AutoTurnControlSpeedTurn":"35","NNFF":"0","NNFFLite":"0","AutoEngage":"1"}"#;

pub(super) fn parameters(fingerprint: &str, mode: &str, settings: &J) -> Result<Vec<u8>> {
    let mut message = if fingerprint == "PSA_PEUGEOT_208" {
        let raw = crate::paramsd_fixture_generate::car_params(0, Some(fingerprint));
        let reader = serialize::read_message(&raw[..], ReaderOptions::new())?;
        let mut message = capnp::message::Builder::new_default();
        message.set_root(reader.get_root::<car_params::Reader<'_>>()?)?;
        message
            .get_root::<car_params::Builder<'_>>()?
            .set_brand("psa");
        message
    } else {
        // Original get_non_essential_params: empty buses/firmware and false
        // alpha_long/is_release/docs. The registry hardcodes true for Hyundai's
        // release argument, so call that already-native constructor directly.
        let private = tempfile::tempdir()?;
        let store = openpilot_params::Params::open(private.path(), "controlsd-fixture")?;
        for (key, value) in entries(settings)? {
            store.put(&key, super::text(&value)?.as_bytes())?;
        }
        let id = Identification {
            candidate: fingerprint.into(),
            observed: (0..8).map(|bus| (bus, Vec::new())).collect(),
            vin: String::new(),
            firmware: Vec::new(),
            source: FingerprintSource::Fixed,
            exact_match: false,
            cached: false,
            vin_rx_address: None,
            vin_rx_bus: None,
            ecu_responses: Vec::new(),
            fw_query_time: 0.,
            packets: 0,
        };
        if vehicle_params::platform(fingerprint)?.brand == "hyundai" {
            openpilot_card::brands::hyundai::parameters::parameters(
                openpilot_card::brands::hyundai::ParamsInput {
                    candidate: fingerprint,
                    fingerprints: &id.observed,
                    firmware: &id.firmware,
                    alpha_long: false,
                    is_release: false,
                    settings: &store,
                },
            )?
        } else {
            openpilot_card::registry::parameters(&id, &store, false)?
        }
    };
    let mut cp = message.get_root::<car_params::Builder<'_>>()?;
    cp.set_openpilot_longitudinal_control(true);
    cp.set_min_steer_speed(0.);
    match mode {
        "pid" => {
            cp.set_steer_control_type(car_params::SteerControlType::Torque);
            let tune = cp.reborrow().get_lateral_tuning().init_pid();
            let capnp::dynamic_value::Builder::Struct(tune) = tune.into() else {
                return Err("PID builder not struct".into());
            };
            wire::fill(
                tune,
                &J::parse(
                    r#"{"kpBP":[0.0,20.0],"kpV":[0.1,0.2],"kiBP":[0.0,20.0],"kiV":[0.02,0.04],"kf":0.0001}"#,
                )?,
            )?;
        }
        "torque" => {
            cp.set_steer_control_type(car_params::SteerControlType::Torque);
            let tune = cp.reborrow().get_lateral_tuning().init_torque();
            let capnp::dynamic_value::Builder::Struct(tune) = tune.into() else {
                return Err("torque builder not struct".into());
            };
            wire::fill(
                tune,
                &J::parse(
                    r#"{"kp":1.0,"ki":0.1,"kf":1.0,"latAccelFactor":2.5,"latAccelOffset":0.0,"friction":0.1,"useSteeringAngle":true}"#,
                )?,
            )?;
        }
        "angle" => cp.set_steer_control_type(car_params::SteerControlType::Angle),
        _ => {} // Original leaves the base tuning intact for any other mode.
    }
    Ok(serialize::write_message_to_words(&message))
}
pub(super) fn case(
    name: &str,
    fingerprint: &str,
    mode: &str,
    frames: J,
    settings: &J,
    changes: &J,
) -> Result<J> {
    let settings = merge(&J::parse(SETTINGS)?, settings)?;
    let raw = parameters(fingerprint, mode, &settings)?;
    let original = serialize::read_message(&raw[..], ReaderOptions::new())?;
    let mut message = capnp::message::Builder::new_default();
    message.set_root(original.get_root::<car_params::Reader<'_>>()?)?;
    for (name, value) in entries(changes)? {
        let mut cp = message.get_root::<car_params::Builder<'_>>()?;
        if name == "useSteeringAngle" {
            let car_params::lateral_tuning::Torque(torque) =
                cp.reborrow().get_lateral_tuning().which()?
            else {
                return Err("useSteeringAngle requires torque tuning".into());
            };
            let mut torque = torque?;
            let capnp::dynamic_value::Builder::Struct(target) = torque.reborrow().into() else {
                return Err("torque builder not struct".into());
            };
            wire::fill(target, &object(vec![("useSteeringAngle".into(), value)])?)?;
        } else {
            if !["startingState", "steerAtStandstill", "flags"].contains(&name.as_str()) {
                return Err("CarParams change outside actual fixture domain".into());
            }
            let capnp::dynamic_value::Builder::Struct(target) = cp.into() else {
                return Err("CarParams not struct".into());
            };
            wire::fill(target, &object(vec![(name, value)])?)?;
        }
    }
    let mut params = Vec::new();
    for (name, value) in entries(&settings)? {
        params.push((name, json(super::text(&value)?.as_bytes())?));
    }
    params.push((
        "CarParams".into(),
        json(&serialize::write_message_to_words(&message))?,
    ));
    object(vec![
        ("name".into(), J::text(name)),
        ("params".into(), object(params)?),
        ("simulation".into(), json(&false)?),
        ("frames".into(), frames),
    ])
}
// Native packet mutation entrypoint for case() controls, including captured
// arbitrary nested/list Events supplied to change(), lives in wire.rs.
