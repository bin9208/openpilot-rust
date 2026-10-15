use super::*;
use crate::{
    callbacks::{self, AlertParams},
    car_specific::CarSpecificParams,
    controller::{
        config::{Config, Mode},
        effects::Personality,
    },
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_logging::Fields;
use openpilot_messaging::state::Options;
use openpilot_msgq::VisionStream;

#[derive(Default)]
struct Params {
    now: f64,
    writes: Vec<(String, bool)>,
    removed: Vec<String>,
    offroad: Vec<String>,
    durable: ImpactStore,
    volatile: ImpactStore,
}
impl AlertParams for Params {
    fn text(&mut self, _: &str) -> Result<Option<String>, callbacks::Error> {
        Ok(None)
    }
    fn integer(&mut self, _: &str) -> Result<i32, callbacks::Error> {
        Ok(2)
    }
    fn boolean(&mut self, _: &str) -> Result<bool, callbacks::Error> {
        Ok(false)
    }
}
impl CarSpecificParams for Params {
    type Error = callbacks::Error;
    fn get_bool(&mut self, _: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error> {
        self.writes.push((key.into(), value));
        Ok(())
    }
}
impl Effects for Params {
    fn impact_update(
        &mut self,
        impact: &mut openpilot_impact_dashcam::ImpactDashcam,
        now: f64,
        allowed: bool,
        trigger: bool,
        quiet: Option<bool>,
        details: serde_json::Value,
    ) -> Result<(), Error> {
        impact.update(
            now,
            allowed,
            trigger,
            quiet,
            details,
            &mut self.durable,
            &mut self.volatile,
        )?;
        Ok(())
    }
    fn presence(&mut self, _: &str) -> Result<bool, Error> {
        Ok(false)
    }
    fn wide_camera(&mut self) -> Result<bool, Error> {
        Ok(false)
    }
    fn personality(&mut self) -> Result<Personality, Error> {
        Ok(Personality::standard())
    }
    fn remove(&mut self, key: &str) -> Result<(), Error> {
        self.removed.push(key.into());
        Ok(())
    }
    fn offroad(&mut self, key: &str, _: Option<&str>) -> Result<(), Error> {
        self.offroad.push(key.into());
        Ok(())
    }
    fn event(&mut self, _: &str, _: Fields) -> Result<(), Error> {
        Ok(())
    }
    fn monotonic(&mut self) -> f64 {
        self.now
    }
    fn timestamp(&mut self) -> Result<u64, Error> {
        Ok((self.now * 1e9) as u64)
    }
    fn available_streams(&mut self) -> Result<Vec<VisionStream>, Error> {
        Ok(vec![])
    }
}
fn fixture() -> (Controller, Params, State) {
    let mut cp = capnp::message::Builder::new_default();
    cp.init_root::<car_params::Builder>();
    let config = Config::read(cp.get_root_as_reader().unwrap()).unwrap();
    let mode = Mode {
        replay: false,
        simulation: false,
        testing_closet: false,
        device_type: "tici".into(),
        nvme_present: false,
        branch: "dev-md".into(),
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/selfdrive/ui/translations");
    let language = openpilot_ui_framework::multilang::Multilang::new(&root, Some("en")).unwrap();
    let mut params = Params {
        now: 1.,
        ..Params::default()
    };
    let mut controller = Controller::new(config, (mode, language), &mut params).unwrap();
    controller.events.clear();
    params.removed.clear();
    let state = State::new(
        &["driverMonitoringState"],
        Options {
            ignore_frequency: vec!["driverMonitoringState".into()],
            ..Options::default()
        },
    )
    .unwrap();
    (controller, params, state)
}
fn packet(now: f64, disabled: bool, locked: bool) -> Vec<u8> {
    let mut m = capnp::message::Builder::new_default();
    let mut root = m.init_root::<log_capnp::event::Builder>();
    root.set_valid(true);
    root.set_log_mono_time((now * 1e9) as u64);
    let mut dm = root.init_driver_monitoring_state();
    dm.set_dm2_disabled(disabled);
    dm.set_lockout(locked);
    dm.set_alert_level(log_capnp::driver_monitoring_state::AlertLevel::Two);
    dm.init_vision_policy_state()
        .set_uncertain_offroad_alert_percent(100);
    capnp::serialize::write_message_to_words(&m)
}
#[test]
fn dm_published_mode_overrides_retired_disable_setting_and_persists_lock_and_release() {
    let (mut c, mut effects, mut state) = fixture();
    state.update(1., &[packet(1., false, true)]).unwrap();
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert!(c.events.names().contains(&E::TooDistracted));
    assert_eq!(effects.writes, vec![("DriverTooDistracted".into(), true)]);
    effects.now = 1.05;
    state.update(1.05, &[packet(1.05, false, false)]).unwrap();
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert_eq!(
        effects.writes.last(),
        Some(&("DriverTooDistracted".into(), false))
    );
}
#[test]
fn disabling_dm_clears_uncertainty_once_and_suppresses_attention_events() {
    let (mut c, mut effects, mut state) = fixture();
    state.update(1., &[packet(1., false, false)]).unwrap();
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert!(c.dm_uncertain_alerted);
    c.events.clear();
    state.update(1.05, &[packet(1.05, true, true)]).unwrap();
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert!(!c.dm_uncertain_alerted);
    assert!(c.events.names().is_empty());
    assert_eq!(effects.removed, vec!["Offroad_DriverMonitoringUncertain"]);
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert_eq!(effects.removed.len(), 1);
}
#[test]
fn stale_dm_lockout_cannot_write_persistent_state() {
    let (mut c, mut effects, mut state) = fixture();
    effects.now = 2.;
    state.update(2., &[packet(1., false, true)]).unwrap();
    c.driver_monitoring(&state, &mut effects).unwrap();
    assert!(effects.writes.is_empty());
}
#[test]
fn driver_camera_is_not_a_controls_readiness_dependency_but_dm_service_is() {
    let (c, _, _) = fixture();
    let (names, options) = c.subscription();
    assert!(!names.contains(&"driverCameraState"));
    assert!(names.contains(&"driverMonitoringState"));
    assert!(!options
        .ignore_alive
        .iter()
        .any(|s| s == "driverMonitoringState"));
    for wide in [false, true] {
        for retired in [0, 1, 2] {
            for simulation in [false, true] {
                assert!(!crate::helpers::camera_packets(wide, retired, simulation)
                    .contains(&"driverCameraState"));
            }
        }
    }
}
#[test]
fn healthy_fallback_excuses_only_the_driver_model_process() {
    let (mut c, mut effects, _) = fixture();
    let mut names = super::super::BASE_TOPICS.to_vec();
    names.push("roadCameraState");
    let ignore: Vec<_> = names
        .iter()
        .filter(|s| **s != "driverMonitoringState")
        .map(|s| s.to_string())
        .collect();
    let mut state = State::new(
        &names,
        Options {
            ignore_alive: ignore.clone(),
            ignore_valid: ignore.clone(),
            ignore_frequency: names.iter().map(|s| s.to_string()).collect(),
            ..Options::default()
        },
    )
    .unwrap();
    let mut dm = capnp::message::Builder::new_default();
    let mut root = dm.init_root::<log_capnp::event::Builder>();
    root.set_valid(true);
    root.set_log_mono_time(1_000_000_000);
    root.init_driver_monitoring_state()
        .set_camera_unavailable(true);
    let mut manager = capnp::message::Builder::new_default();
    let mut root = manager.init_root::<log_capnp::event::Builder>();
    root.set_valid(true);
    let mut p = root.init_manager_state().init_processes(1).get(0);
    p.set_name("dmonitoringmodeld");
    p.set_running(false);
    p.set_should_be_running(true);
    let bytes = vec![
        capnp::serialize::write_message_to_words(&dm),
        capnp::serialize::write_message_to_words(&manager),
    ];
    state.update(1., &bytes).unwrap();
    state.update(1.01, &bytes).unwrap();
    let cs = crate::controller::car_frame::CarFrame::initial().unwrap();
    c.health_events(cs.state().unwrap(), &state, false, &mut effects)
        .unwrap();
    assert!(!c.events.names().contains(&E::ProcessNotRunning));
    c.events.clear();
    let mut root = dm.get_root::<log_capnp::event::Builder>().unwrap();
    root.set_valid(false);
    state
        .update(1.02, &[capnp::serialize::write_message_to_words(&dm)])
        .unwrap();
    c.health_events(cs.state().unwrap(), &state, false, &mut effects)
        .unwrap();
    assert!(c.events.names().contains(&E::ProcessNotRunning));
}
#[test]
fn final_source_dm_consumer_trace_matches_events_persistence_and_disabled_edges() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/dm2-consumer-final.json")).unwrap();
    let (mut controller, mut effects, mut state) = fixture();
    for input in reference["cases"].as_array().unwrap() {
        let now = input["now"].as_f64().unwrap();
        effects.now = now;
        let mut m = capnp::message::Builder::new_default();
        let mut root = m.init_root::<log_capnp::event::Builder>();
        root.set_valid(input["valid"].as_bool().unwrap());
        root.set_log_mono_time((input["stamp"].as_f64().unwrap() * 1e9).round() as u64);
        let mut dm = root.init_driver_monitoring_state();
        dm.set_dm2_disabled(input["disabled"].as_bool().unwrap());
        dm.set_lockout(input["locked"].as_bool().unwrap());
        dm.set_always_on_lockout(input["always"].as_bool().unwrap());
        use log_capnp::driver_monitoring_state::{AlertLevel, MonitoringPolicy};
        dm.set_alert_level(match input["level"].as_str().unwrap() {
            "one" => AlertLevel::One,
            "two" => AlertLevel::Two,
            "three" => AlertLevel::Three,
            _ => AlertLevel::None,
        });
        dm.set_active_policy(if input["policy"] == "vision" {
            MonitoringPolicy::Vision
        } else {
            MonitoringPolicy::Wheeltouch
        });
        dm.init_vision_policy_state()
            .set_uncertain_offroad_alert_percent(input["uncertain"].as_i64().unwrap() as i8);
        state
            .update(now, &[capnp::serialize::write_message_to_words(&m)])
            .unwrap();
        controller.events.clear();
        controller.driver_monitoring(&state, &mut effects).unwrap();
        let expected: Vec<u16> = input["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        let mut expected = expected;
        expected.sort_unstable();
        let actual: Vec<u16> = controller
            .events
            .names()
            .iter()
            .map(|e| u16::from(*e))
            .collect();
        assert_eq!(actual, expected, "events {now}");
        assert_eq!(
            serde_json::to_value(&effects.writes).unwrap(),
            input["writes"],
            "writes {now}"
        );
        assert_eq!(
            serde_json::to_value(&effects.offroad).unwrap(),
            input["shown"]
        );
        assert_eq!(
            serde_json::to_value(&effects.removed).unwrap(),
            input["removed"]
        );
        assert_eq!(
            controller.dm_lockout_set,
            input["lockout_set"].as_bool().unwrap()
        );
        assert_eq!(
            controller.dm_uncertain_alerted,
            input["uncertain_alerted"].as_bool().unwrap()
        );
        assert_eq!(
            controller.dm_disabled_previous,
            input["disabled_previous"].as_bool().unwrap()
        );
    }
}
#[test]
fn unhealthy_startup_keeps_ten_second_model_allowance() {
    let (mut controller, mut effects, _) = fixture();
    controller.initialized = false;
    let (names, options) = controller.subscription();
    let names: Vec<String> = names.into_iter().map(str::to_owned).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut state = State::new(&refs, options).unwrap();
    for frame in 0..602 {
        state.update(frame as f64 * 0.01, &[]).unwrap();
    }
    let cs = crate::controller::car_frame::CarFrame::initial().unwrap();
    controller
        .data_sample(cs.state().unwrap(), &mut state, &mut effects)
        .unwrap();
    assert!(
        !controller.initialized,
        "unhealthy model startup must retain the final-source ten-second allowance"
    );
}

#[test]
fn radar_warning_input_is_optional_and_meb_distance_delay_is_separate() {
    let (controller, _, _) = fixture();
    let (names, options) = controller.subscription();
    assert!(names.contains(&"liveTracks"));
    for ignores in [
        &options.ignore_alive,
        &options.ignore_valid,
        &options.ignore_frequency,
    ] {
        assert!(ignores.iter().any(|n| n == "liveTracks"));
    }
    for (brand, flags, delay, expected) in [
        ("volkswagen", 16, 0.8, 0.),
        ("volkswagen", 0, 0.8, 0.8),
        ("hyundai", 16, 0.3, 0.3),
    ] {
        let mut cp = capnp::message::Builder::new_default();
        let mut value = cp.init_root::<car_params::Builder>();
        value.set_brand(brand);
        value.set_flags(flags);
        value.set_radar_delay(delay);
        let config = Config::read(cp.get_root_as_reader().unwrap()).unwrap();
        assert!((config.front_radar_delay - expected).abs() < 1e-7);
    }
}

#[derive(Default)]
struct ImpactStore(std::collections::BTreeMap<String, Vec<u8>>);
impl openpilot_impact_dashcam::Store for ImpactStore {
    fn get(&mut self, key: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.0.get(key).cloned())
    }
    fn put(&mut self, key: &str, value: &[u8]) -> std::io::Result<()> {
        self.0.insert(key.into(), value.into());
        Ok(())
    }
    fn remove(&mut self, key: &str) -> std::io::Result<()> {
        self.0.remove(key);
        Ok(())
    }
}
#[test]
fn impact_imu_runtime_gate_rejects_invalid_pose_replay_and_stale_sensors() {
    use openpilot_impact_dashcam::ImpactDashcam;
    for blocked in 0..7 {
        let (mut c, mut p, _) = fixture();
        c.initialized = true;
        c.impact = Some(ImpactDashcam::new(&mut p.durable, &mut p.volatile).unwrap());
        match blocked {
            1 => c.mode.replay = true,
            2 => c.mode.simulation = true,
            3 => c.config.not_car = true,
            4 => c.config.passive = true,
            _ => {}
        }
        let names = [
            "accelerometer",
            "livePose",
            "liveCalibration",
            "deviceState",
        ];
        let mut s = State::new(
            &names,
            Options {
                ignore_frequency: names.iter().map(|n| (*n).into()).collect(),
                ..Options::default()
            },
        )
        .unwrap();
        for (now, stamp) in [(1., 1.), (1.01, 1.01)] {
            p.now = now;
            let mut sensor = capnp::message::Builder::new_default();
            let mut e = sensor.init_root::<log_capnp::event::Builder>();
            e.set_valid(true);
            e.set_log_mono_time((now * 1e9) as u64);
            let mut a = e.init_accelerometer();
            a.set_timestamp((if blocked == 6 { stamp - 1. } else { stamp } * 1e9) as i64);
            a.set_source(log_capnp::sensor_event_data::SensorSource::Lsm6ds3);
            let mut v = a.init_acceleration().init_v(3);
            v.set(0, 9.81);
            v.set(1, 0.);
            v.set(2, -16.);
            let mut pose_message = capnp::message::Builder::new_default();
            let mut e = pose_message.init_root::<log_capnp::event::Builder>();
            e.set_valid(true);
            e.set_log_mono_time((now * 1e9) as u64);
            let mut pose = e.init_live_pose();
            pose.set_timestamp((now * 1e9) as u64);
            pose.set_inputs_o_k(true);
            pose.set_sensors_o_k(true);
            pose.set_posenet_o_k(true);
            pose.init_orientation_n_e_d().set_valid(blocked != 5);
            let mut calibration = capnp::message::Builder::new_default();
            let mut e = calibration.init_root::<log_capnp::event::Builder>();
            e.set_valid(true);
            let mut cal = e.init_live_calibration();
            cal.set_cal_status(log_capnp::live_calibration_data::Status::Calibrated);
            cal.init_rpy_calib(3);
            let mut device = capnp::message::Builder::new_default();
            let mut e = device.init_root::<log_capnp::event::Builder>();
            e.set_valid(true);
            e.init_device_state().set_started(true);
            s.update(
                now,
                &[sensor, pose_message, calibration, device]
                    .iter()
                    .map(capnp::serialize::write_message_to_words)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let car = crate::controller::car_frame::CarFrame::initial().unwrap();
            c.update_impact(car.state().unwrap(), &s, &mut p).unwrap();
        }
        assert_eq!(
            c.impact.as_ref().unwrap().pending,
            blocked == 0,
            "gate {blocked}"
        );
    }
}
