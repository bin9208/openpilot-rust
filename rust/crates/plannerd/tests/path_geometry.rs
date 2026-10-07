use openpilot_plannerd::path_geometry::yaw_from_path;

#[test]
fn straight_path_produces_zero_yaw_and_yaw_rate() {
    // Given: a straight path across the complete model horizon.
    let path = std::array::from_fn(|index| [f64::from(u32::try_from(index).unwrap()), 0., 0.]);
    // When: reconstructing the lateral plan at normal road speed.
    let output = yaw_from_path(&path, &[20.; 33]).unwrap();
    // Then: the path requires no turn or turn-rate correction.
    assert_eq!(output.yaw, [0.; 33]);
    assert_eq!(output.rate, [0.; 33]);
}

#[test]
fn nonfinite_path_derivatives_are_zeroed_as_in_source() {
    // Given: a path whose longitudinal coordinates are all nonfinite.
    let path = [[f64::NAN, 0., 0.]; 33];
    // When: reconstructing its low-speed lateral geometry.
    let output = yaw_from_path(&path, &[1.; 33]).unwrap();
    // Then: invalid derivatives cannot escape the source stabilization boundary.
    assert_eq!(output.yaw, [0.; 33]);
    assert_eq!(output.rate, [0.; 33]);
}
