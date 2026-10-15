#![cfg(feature = "native-skip-miri")]

use openpilot_params::Params;
use openpilot_plannerd::{native_parameters::RuntimeParameters, parameters::Parameters};
use std::fs;

#[test]
fn filesystem_read_errors_keep_source_empty_defaults_and_retry_after_recovery() {
    let root = tempfile::tempdir().unwrap();
    let mut parameters = RuntimeParameters(Params::open(root.path(), "fixture").unwrap());
    for key in ["EnableRadarTracks", "LatMpcPathCost"] {
        fs::create_dir(root.path().join("fixture").join(key)).unwrap();
    }
    assert_eq!(parameters.integer("EnableRadarTracks").unwrap(), 0);
    assert_eq!(parameters.float("LatMpcPathCost").unwrap(), 0.0);
    for key in ["EnableRadarTracks", "LatMpcPathCost"] {
        fs::remove_dir(root.path().join("fixture").join(key)).unwrap();
    }
    parameters.0.put("EnableRadarTracks", b"2").unwrap();
    parameters.0.put("LatMpcPathCost", b"1.25").unwrap();
    assert_eq!(parameters.integer("EnableRadarTracks").unwrap(), 2);
    assert_eq!(parameters.float("LatMpcPathCost").unwrap(), 1.25);
    parameters.0.put("EnableRadarTracks", b"invalid").unwrap();
    parameters.0.put("LatMpcPathCost", b"invalid").unwrap();
    assert!(parameters.integer("EnableRadarTracks").is_err());
    assert!(parameters.float("LatMpcPathCost").is_err());
    assert!(parameters.integer("UnknownPlannerFixtureKey").is_err());
}

#[test]
fn stopping_threshold_is_bounded_immediately_and_repairs_external_numeric_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut parameters = RuntimeParameters(Params::open(root.path(), "stopping").unwrap());
    for (raw, expected, repaired) in [
        ("-10", 0.1, "10"),
        ("invalid", 0.5, "50"),
        ("NaN", 0.5, "50"),
        ("75", 0.75, "75"),
    ] {
        parameters.0.put("VEgoStopping", raw.as_bytes()).unwrap();
        assert_eq!(parameters.stopping_speed().unwrap(), expected);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while parameters.0.get("VEgoStopping").unwrap().unwrap() != repaired.as_bytes() {
            assert!(std::time::Instant::now() < deadline, "repair {raw}");
            parameters.stopping_speed().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
