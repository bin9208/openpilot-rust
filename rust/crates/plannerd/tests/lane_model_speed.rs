use openpilot_plannerd::lane_model_speed::LaneModelSpeedGuard;

#[test]
fn shortened_increasing_horizon_requires_continuous_recovery() {
    let mut guard = LaneModelSpeedGuard::new(20);
    assert!(!guard.update(30., 10., 15.));
    for _ in 0..20 {
        assert!(!guard.update(30., 21., 14.7));
    }
    assert!(guard.update(30., 21., 14.7));
    assert!(!guard.update(30., 20.99, 25.));
    assert!(!guard.update(30., 21., 14.69));
    for _ in 0..21 {
        guard.update(30., 30., 30.);
    }
    assert!(guard.update(30., 30., 30.));
}

#[test]
fn invalid_each_speed_resets_recovery() {
    for bad in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for index in 0..3 {
            let mut guard = LaneModelSpeedGuard::new(0);
            assert!(guard.update(30., 30., 30.));
            let mut values = [30.; 3];
            values[index] = bad;
            assert!(!guard.update(values[0], values[1], values[2]));
        }
    }
}
