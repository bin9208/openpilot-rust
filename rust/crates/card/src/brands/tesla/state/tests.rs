//! Ported from carrot-wip `opendbc/car/tesla/tests/test_dbc_decoding.py`.
use super::*;
use crate::query::DiagnosticLevel;
use openpilot_can::{packer::Packer, Frame};

const N: u64 = 5_000_000_000;
type Fields = &'static [(&'static str, f64)];
const SPEED: Fields = &[
    ("DI_vehicleSpeed", 20.),
    ("DI_uiSpeed", 20.),
    ("DI_uiSpeedUnits", 1.),
];
const CRUISE: Fields = &[
    ("DI_digitalSpeed", 20.),
    ("DI_speedUnits", 1.),
    ("DI_cruiseState", 2.),
];
const SYSTEM: Fields = &[("DI_accelPedalPos", 0.), ("DI_gear", 4.)];
const WHEELS: Fields = &[
    ("ESP_wheelSpeedFrL", 20.),
    ("ESP_wheelSpeedFrR", 20.),
    ("ESP_wheelSpeedReL", 20.),
    ("ESP_wheelSpeedReR", 20.),
];
const HEALTHY: [(&str, Fields); 4] = [
    ("DI_speed", SPEED),
    ("DI_state", CRUISE),
    ("DI_systemStatus", SYSTEM),
    ("ESP_wheelSpeeds", WHEELS),
];

fn healthy(message: &str) -> Fields {
    HEALTHY.iter().find(|(name, _)| *name == message).unwrap().1
}
struct Vehicle {
    state: State,
    party: Packer,
    body: Packer,
}
impl Vehicle {
    fn new(flags: u32) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
        let state = State::new(&root, flags, 15., true, 0).unwrap();
        let party = Packer::new(Arc::clone(&state.party.dbc));
        let body = Packer::new(Arc::new(
            Dbc::load(&root.join("tesla_modely_hw4_perception.dbc")).unwrap(),
        ));
        let mut vehicle = Self { state, party, body };
        vehicle.step(Vec::new(), N, N);
        let frames = HEALTHY
            .iter()
            .map(|(name, fields)| vehicle.frame(name, fields, &[], 0))
            .collect();
        vehicle.step(frames, N, N);
        vehicle
    }
    fn frame(&mut self, name: &str, base: Fields, changes: &[(&str, f64)], bus: u8) -> Frame {
        let mut values = base.to_vec();
        for (signal, value) in changes {
            match values.iter_mut().find(|(name, _)| name == signal) {
                Some(entry) => entry.1 = *value,
                None => values.push((signal, *value)),
            }
        }
        Frame {
            address: self.party.dbc.message(name).unwrap().address,
            data: self.party.pack(name, &values, None).unwrap(),
            bus,
        }
    }
    fn tpms(&mut self, pressure: f64, bus: u8) -> Frame {
        Frame {
            address: 0x25a,
            data: self
                .body
                .pack(
                    "VCSEC_TPMSDisplay",
                    &[("VCSEC_TPMSDisplayPressureFL", pressure)],
                    None,
                )
                .unwrap(),
            bus,
        }
    }
    /// One update with every frame stamped `stamp`, evaluated at `now`.
    fn step(&mut self, frames: Vec<Frame>, stamp: u64, now: u64) -> Message {
        let packets = if frames.is_empty() {
            Vec::new()
        } else {
            vec![Packet {
                mono_time: stamp,
                frames,
            }]
        };
        self.state.update(&packets, now).unwrap()
    }
    fn poke(&mut self, message: &str, signal: &str, value: f64) {
        let msg = self.state.party.dbc.message(message).unwrap();
        let index = msg.signals.iter().position(|s| s.name == signal).unwrap();
        let address = msg.address;
        self.state.party.states.get_mut(&address).unwrap().values[index] = value;
    }
}
fn reader(message: &Message) -> car_state::Reader<'_> {
    message.get_root_as_reader().unwrap()
}
fn close(actual: f32, expected: f64, tolerance: f64) -> bool {
    (f64::from(actual) - expected).abs() <= tolerance
}
/// The speed frame's checksum after a manual payload edit: address bytes plus
/// every byte except the checksum itself.
fn speed_checksum(data: &mut [u8]) {
    let sum = 0x57 + 2 + data[1..].iter().map(|b| u32::from(*b)).sum::<u32>();
    data[0] = u8::try_from(sum & 255).unwrap();
}

#[test]
fn vehicle_bus_state_starts_with_the_separate_tpms_parser() {
    let vehicle = Vehicle::new(HAS_VEHICLE_BUS);
    assert_eq!(
        vehicle.state.vehicle.as_ref().unwrap().dbc.name,
        "tesla_model3_vehicle"
    );
    let body = vehicle.state.body.as_ref().unwrap();
    assert_eq!(body.dbc.name, "tesla_modely_hw4_perception");
    assert!(body.states.contains_key(&0x25a));
    assert!(Vehicle::new(0).state.body.is_none());
}

#[test]
fn display_units_use_bit33_without_speed_based_guessing() {
    for units in [0., 1.] {
        for speed in [0., 5., 90.] {
            for reserved in [0_u8, 1] {
                let mut vehicle = Vehicle::new(0);
                let mut frame = vehicle.frame(
                    "DI_speed",
                    SPEED,
                    &[
                        ("DI_vehicleSpeed", speed),
                        ("DI_uiSpeed", speed),
                        ("DI_uiSpeedUnits", units),
                    ],
                    0,
                );
                frame.data[4] = (frame.data[4] & !1) | reserved;
                speed_checksum(&mut frame.data);
                assert_eq!(f64::from((frame.data[4] >> 1) & 1), units);
                let message = vehicle.step(vec![frame], N, N);
                let result = reader(&message);
                let factor = if units == 1. { 1. / 3.6 } else { 0.44704 };
                assert!(
                    close(result.get_v_ego_cluster(), speed * factor, 0.1),
                    "units {units} speed {speed}: {}",
                    result.get_v_ego_cluster()
                );
                assert!(!result.get_vehicle_sensors_invalid());
            }
        }
    }
}

#[test]
fn reserved_measurements_hold_last_value_and_report_invalid_until_recovery() {
    type Read = fn(car_state::Reader<'_>) -> f32;
    let cases: [(&str, &str, f64, Read); 7] = [
        ("DI_speed", "DI_vehicleSpeed", 287.6, |s| s.get_v_ego_raw()),
        ("DI_speed", "DI_uiSpeed", 255., |s| s.get_v_ego_cluster()),
        ("ESP_wheelSpeeds", "ESP_wheelSpeedFrL", 327.64, |s| {
            s.get_wheel_speeds().unwrap().get_fl()
        }),
        ("ESP_wheelSpeeds", "ESP_wheelSpeedFrR", 327.64, |s| {
            s.get_wheel_speeds().unwrap().get_fr()
        }),
        ("ESP_wheelSpeeds", "ESP_wheelSpeedReL", 327.64, |s| {
            s.get_wheel_speeds().unwrap().get_rl()
        }),
        ("ESP_wheelSpeeds", "ESP_wheelSpeedReR", 327.64, |s| {
            s.get_wheel_speeds().unwrap().get_rr()
        }),
        ("DI_systemStatus", "DI_accelPedalPos", 102., |s| s.get_gas()),
    ];
    for (message, signal, value, read) in cases {
        let mut vehicle = Vehicle::new(0);
        vehicle.state.logs.clear();
        let previous = read(reader(&vehicle.state.out));
        let frame = vehicle.frame(message, healthy(message), &[(signal, value)], 0);
        let update = vehicle.step(vec![frame], N, N);
        let result = reader(&update);
        assert!(result.get_vehicle_sensors_invalid(), "{message}.{signal}");
        assert_eq!(read(result), previous, "{message}.{signal}");
        if message == "DI_systemStatus" {
            assert!(result.get_gas_pressed());
        }
        for _ in 0..5 {
            let update = vehicle.step(Vec::new(), N, N);
            assert!(reader(&update).get_vehicle_sensors_invalid());
        }
        let errors: Vec<_> = vehicle
            .state
            .logs
            .iter()
            .filter(|log| matches!(log.level, DiagnosticLevel::Error))
            .collect();
        assert_eq!(errors.len(), 1, "{message}.{signal}");
        assert!(errors[0].message.contains(&format!("{message}.{signal}")));
        let frame = vehicle.frame(message, healthy(message), &[], 0);
        let update = vehicle.step(vec![frame], N, N);
        assert!(!reader(&update).get_vehicle_sensors_invalid());
        assert!(vehicle
            .state
            .logs
            .iter()
            .any(|log| log.message == "Tesla measurements recovered"));
    }
}

#[test]
fn nonfinite_or_out_of_range_measurements_fail_closed() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 9999.] {
        for (message, signal) in [
            ("DI_speed", "DI_vehicleSpeed"),
            ("DI_speed", "DI_uiSpeed"),
            ("ESP_wheelSpeeds", "ESP_wheelSpeedFrL"),
            ("DI_state", "DI_digitalSpeed"),
        ] {
            let mut vehicle = Vehicle::new(0);
            vehicle.poke(message, signal, value);
            let update = vehicle.step(Vec::new(), N, N);
            assert!(
                reader(&update).get_vehicle_sensors_invalid(),
                "{message}.{signal} = {value}"
            );
        }
    }
}

#[test]
fn invalid_vehicle_speed_never_advances_speed_filter() {
    for truncated in [false, true] {
        let mut vehicle = Vehicle::new(0);
        let held = reader(&vehicle.state.out).get_v_ego();
        let mut frame = vehicle.frame("DI_speed", SPEED, &[("DI_vehicleSpeed", 287.6)], 0);
        if truncated {
            frame.data.truncate(5);
            speed_checksum(&mut frame.data);
        }
        let update = vehicle.step(vec![frame], N, N);
        let result = reader(&update);
        assert!(result.get_vehicle_sensors_invalid());
        assert!(close(result.get_v_ego_raw(), 20. / 3.6, 1e-5));
        assert_eq!(result.get_v_ego(), held);
        assert_eq!(result.get_a_ego(), 0.);
        // The filter saw exactly one sample (the healthy one): the next valid
        // sample advances it the way a second sample would on a fresh filter.
        let mut fresh = SpeedFilter::new().unwrap();
        let raw = f64::from((20.0_f64 / 3.6) as f32);
        fresh.update(raw);
        let [expected, _] = fresh.update(raw);
        let frame = vehicle.frame("DI_speed", SPEED, &[], 0);
        let update = vehicle.step(vec![frame], N, N);
        assert_eq!(reader(&update).get_v_ego(), expected as f32);
    }
}

#[test]
fn fused_speed_limit_has_fixed_kph_units() {
    for units in [0., 1.] {
        let mut vehicle = Vehicle::new(0);
        let frames = vec![
            vehicle.frame("DI_state", CRUISE, &[("DI_speedUnits", units)], 0),
            vehicle.frame("DAS_status", &[], &[("DAS_fusedSpeedLimit", 60.)], 2),
        ];
        let update = vehicle.step(frames, N, N);
        assert_eq!(reader(&update).get_speed_limit(), 60.);
        assert!((vehicle.state.extras.tesla_speed_limit_target - 60. / 3.6).abs() < 1e-9);
        assert!(vehicle.state.extras.tesla_speed_limit_target_valid);
        assert_eq!(
            vehicle.state.extras.tesla_speed_units,
            if units == 1. { "KPH" } else { "MPH" }
        );
    }
}

#[test]
fn digital_pedal_press_cannot_be_hidden_by_analog_noise_filter() {
    for (pressed, position, expected) in [(1., 0.4, true), (0., 0.4, false), (0., 1.2, true)] {
        let mut vehicle = Vehicle::new(0);
        let frames = vec![
            vehicle.frame("DI_speed", SPEED, &[("DI_accelPedalPressed", pressed)], 0),
            vehicle.frame(
                "DI_systemStatus",
                SYSTEM,
                &[("DI_accelPedalPos", position)],
                0,
            ),
        ];
        let update = vehicle.step(frames, N, N);
        let result = reader(&update);
        assert_eq!(result.get_gas_pressed(), expected, "{pressed} {position}");
        assert!(!result.get_vehicle_sensors_invalid());
    }
}

#[test]
fn future_or_stale_digital_pedal_flag_is_not_replayed() {
    for stamp in [N + 1, N - 500_000_001] {
        let mut vehicle = Vehicle::new(0);
        let frame = vehicle.frame("DI_speed", SPEED, &[("DI_accelPedalPressed", 1.)], 0);
        let update = vehicle.step(vec![frame], stamp, N);
        assert!(!reader(&update).get_gas_pressed(), "stamp {stamp}");
    }
}

#[test]
fn digital_pedal_release_preserves_only_analog_hysteresis() {
    for (initial, expected_after_release) in [(0.4, false), (1.2, true)] {
        let mut vehicle = Vehicle::new(0);
        let frames = vec![
            vehicle.frame("DI_speed", SPEED, &[("DI_accelPedalPressed", 1.)], 0),
            vehicle.frame(
                "DI_systemStatus",
                SYSTEM,
                &[("DI_accelPedalPos", initial)],
                0,
            ),
        ];
        assert!(reader(&vehicle.step(frames, N, N)).get_gas_pressed());
        let frames = vec![
            vehicle.frame("DI_speed", SPEED, &[], 0),
            vehicle.frame("DI_systemStatus", SYSTEM, &[("DI_accelPedalPos", 0.8)], 0),
        ];
        assert_eq!(
            reader(&vehicle.step(frames, N, N)).get_gas_pressed(),
            expected_after_release
        );
        let released = vehicle.frame("DI_systemStatus", SYSTEM, &[], 0);
        assert!(!reader(&vehicle.step(vec![released], N, N)).get_gas_pressed());
    }
}

#[test]
fn separate_hw4_tpms_parser_uses_vehicle_bus_without_false_bus_timeout() {
    let mut vehicle = Vehicle::new(HAS_VEHICLE_BUS);
    let frame = vehicle.tpms(2.6, 1);
    let update = vehicle.step(vec![frame], N, N);
    assert!(close(
        reader(&update).get_tpms().unwrap().get_fl(),
        37.7,
        0.05
    ));
    assert!(!vehicle.state.body.as_ref().unwrap().bus_timeout());
    let quiet = [Packet {
        mono_time: N + 600_000_000,
        frames: Vec::new(),
    }];
    vehicle.state.update(&quiet, N + 600_000_000).unwrap();
    let body = vehicle.state.body.as_mut().unwrap();
    assert!(!body.bus_timeout());
    assert!(body.can_valid());
    assert!(vehicle
        .state
        .logs
        .iter()
        .any(|log| log.message == "Tesla TPMS data available"));
}

#[test]
fn tpms_ignores_unrelated_sources_and_transmit_echoes() {
    for bus in [0, 2, 0x81, 0xc1] {
        let mut vehicle = Vehicle::new(HAS_VEHICLE_BUS);
        let frame = vehicle.tpms(2.6, bus);
        let update = vehicle.step(vec![frame], N, N);
        assert_eq!(reader(&update).get_tpms().unwrap().get_fl(), 0., "{bus}");
    }
}

#[test]
fn tpms_stale_or_invalid_data_is_not_displayed() {
    for invalid in ["stale", "future", "short", "long", "sna"] {
        let mut vehicle = Vehicle::new(HAS_VEHICLE_BUS);
        let frame = vehicle.tpms(2.6, 1);
        let update = vehicle.step(vec![frame.clone()], N, N);
        assert!(close(
            reader(&update).get_tpms().unwrap().get_fl(),
            37.7,
            0.05
        ));
        let update = match invalid {
            "stale" => vehicle.step(Vec::new(), N, N + 10_000_000_001),
            "future" => vehicle.step(vec![frame], N + 1, N),
            _ => {
                let mut frame = frame;
                match invalid {
                    "short" => frame.data.truncate(6),
                    "long" => frame.data.push(0),
                    _ => frame.data[0] = 0xff,
                }
                vehicle.step(vec![frame], N, N)
            }
        };
        assert_eq!(
            reader(&update).get_tpms().unwrap().get_fl(),
            0.,
            "{invalid}"
        );
    }
}
