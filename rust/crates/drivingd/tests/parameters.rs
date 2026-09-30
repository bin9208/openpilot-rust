use openpilot_driving_modeld::parameters;
use openpilot_params::Params;

#[test]
fn source_numeric_prefix_float32_and_default_boolean_rules() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "test").unwrap();
    assert!(parameters::use_wide(&params).unwrap());
    params.put("UseWideCamera", b"0").unwrap();
    assert!(!parameters::use_wide(&params).unwrap());
    params.put("UseWideCamera", b"").unwrap();
    assert!(parameters::use_wide(&params).unwrap());
    for (input, value) in [
        (b" 12.34567rest".as_slice(), 12.34567_f32),
        (b"0x1.8p2", 6.0),
    ] {
        params.put("CameraYawTrimDeg", input).unwrap();
        assert_eq!(
            parameters::float(&params, "CameraYawTrimDeg").unwrap(),
            f64::from(value)
        );
    }
    params.put("LaneChangeBsd", b" -12rest").unwrap();
    assert_eq!(parameters::integer(&params, "LaneChangeBsd").unwrap(), -12);
    for bytes in [b"1e999".as_slice(), b"abc", b"1e-999"] {
        params.put("CameraYawTrimDeg", bytes).unwrap();
        assert!(parameters::float(&params, "CameraYawTrimDeg").is_err());
    }
    params.put("LaneChangeBsd", b"2147483648").unwrap();
    assert!(parameters::integer(&params, "LaneChangeBsd").is_err());
}
