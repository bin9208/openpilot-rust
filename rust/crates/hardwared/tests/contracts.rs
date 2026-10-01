use openpilot_hardwared::{
    power::{PowerMonitoring, Shutdown},
    wire,
};
use serde_json::json;
#[test]
fn hardware_power_read_failure_retains_voltage_save_and_previous_integration_time() {
    // Given a prior offroad measurement.
    let mut power = PowerMonitoring::new(3e6);
    power.calculate(10., Some(12000.), false, 10.);
    // When the native power observation fails at the next save boundary.
    let (save, error) =
        power.calculate_with(20., Some(10000.), false, || Err("power read failed".into()));
    // Then source pre-read updates persist but energy integration does not advance.
    assert_eq!(save, Some(3_000_000));
    assert_eq!(error.as_deref(), Some("power read failed"));
    assert_eq!(power.last_measurement, Some(10.));
    assert_eq!(power.instant_voltage, 10000.);
    assert!(power.voltage < 12000.);
}
#[test]
fn forced_shutdown_requires_offroad_timestamp_and_minimum_boot_age() {
    // Given force power down without a prior onroad start.
    let power = PowerMonitoring::new(3e6);
    let input = Shutdown {
        now: 3600.,
        ignition: true,
        in_car: false,
        off_ts: Some(1.),
        started_seen: false,
        max_offroad_minutes: 0,
        disable: true,
        force: true,
    };
    // When evaluating the exact uptime boundary.
    let shutdown = power.should_shutdown(&input);
    // Then even force remains blocked until strictly after one hour.
    assert!(!shutdown);
}
#[test]
fn cereal_integer_overflow_is_rejected_instead_of_saturating() {
    // Given an out-of-range native hardware observation.
    let state = json!({"screenBrightnessPercent": 128});
    // When encoding the source's Int8 field.
    let result = wire::device(&state, 1.);
    // Then publication fails rather than silently changing the observation.
    assert!(result.is_err());
}
#[test]
fn status_packet_omits_unset_pointers_and_deprecated_groups() {
    // Given a PC deviceState with no thermal arrays configured.
    let state = json!({"deviceType":"pc", "started":false});
    // When producing the native event and status packet together.
    let (_, packet) = wire::device(&state, 1.).unwrap();
    // Then the Python to_dict/strip_deprecated_keys shape is retained.
    assert!(packet["deviceState"].get("cpuTempC").is_none());
    assert!(packet["deviceState"].get("deprecated").is_none());
    assert_eq!(packet["logMonoTime"], 1_000_000_000_u64);
}
