//! The seven startup inputs in check_paramsd_startup.py. These bytes are
//! compared with seeds intercepted from the unchanged original controller.
use crate::paramsd_fixture_generate::car_params;
use capnp::serialize::write_message_to_words;
use openpilot_cereal::log_capnp::event;
use serde::Serialize;
use std::collections::BTreeMap;

pub const ORIGINAL_SHA256: &str =
    "99d5d76b292d841c9d0e8b78a29e8750679aa297ed0dfea473d829e3877c22eb";
pub const FIXTURE_SHA256: &str = "8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Case {
    pub name: &'static str,
    pub seed: BTreeMap<&'static str, Vec<u8>>,
}

pub fn cases() -> Vec<Case> {
    let base = BTreeMap::from([
        ("CarParams", car_params(0, None)),
        ("CarParamsPrevRoute", car_params(0, None)),
    ]);
    let mut packet = capnp::message::Builder::new_default();
    let mut root = packet.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(1_000_000_000);
    root.set_valid(true);
    let mut parameters = root.init_live_parameters();
    parameters.set_steer_ratio(15.);
    let mut std = parameters.init_debug_filter_state().init_std(2);
    std.set(0, 0.2);
    std.set(1, 0.3);
    let malformed_covariance = write_message_to_words(&packet);
    [
        ("blocked-car", BTreeMap::new()),
        ("malformed-car", BTreeMap::from([("CarParams", vec![1, 2])])),
        ("migration", {
            let mut seed = base.clone();
            seed.insert(
                "LiveParameters",
                br#"{"steerRatio":17,"stiffnessFactor":0.7,"angleOffsetAverageDeg":2}"#.to_vec(),
            );
            seed
        }),
        ("invalid-json", {
            let mut seed = base.clone();
            seed.insert("LiveParameters", b"{".to_vec());
            seed
        }),
        ("wrong-fields", {
            let mut seed = base.clone();
            seed.insert("LiveParameters", b"{}".to_vec());
            seed
        }),
        ("malformed-cache", {
            let mut seed = base.clone();
            seed.insert("LiveParametersV2", vec![1, 2]);
            seed
        }),
        ("wrong-covariance-dimensions", {
            let mut seed = base;
            seed.insert("LiveParametersV2", malformed_covariance);
            seed
        }),
    ]
    .into_iter()
    .map(|(name, seed)| Case { name, seed })
    .collect()
}
