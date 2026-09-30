use openpilot_monitoring::{DriverMonitoring, Input};

#[test]
fn unused_driver_arrays_are_not_indexed_and_selected_short_arrays_fail() {
    let mut input = Input::default();
    input.driver.right.face_orientation = Some(vec![1.]);
    let mut dm = DriverMonitoring::new(false, false, false);
    assert!(dm.run_step(&input).is_ok());
    input.driver.left.face_orientation = Some(vec![1.]);
    assert!(dm.run_step(&input).is_err());
}

#[test]
fn empty_arrays_return_before_coordinate_validation_and_only_used_coordinates_are_required() {
    let mut input = Input::default();
    input.driver.left.face_orientation = Some(vec![1.]);
    input.driver.left.face_position = None;
    input.calibration = vec![];
    let mut dm = DriverMonitoring::new(false, false, false);
    assert!(dm.run_step(&input).is_ok());
    input.driver.left.face_orientation = Some(vec![0., 0.]);
    input.driver.left.face_orientation_std = Some(vec![0., 0.]);
    input.driver.left.face_position = Some(vec![0., 0.]);
    input.driver.left.face_position_std = Some(vec![0.]);
    input.calibration = vec![0., 0., 0., 99.];
    assert!(dm.run_step(&input).is_ok());
    input.calibration.truncate(2);
    assert!(dm.run_step(&input).is_err());
}
