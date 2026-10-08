use openpilot_radard::{
    path::Path,
    point::{velocity_in_ego_frame, Point},
};

#[test]
fn projection_changes_model_lateral_sign_once() {
    let path = Path::new(&[[0., 0.], [20., -2.]]).unwrap();
    let projection = path.project(20., 2.);
    assert_eq!(projection.d_path, 0.);
    assert_eq!(projection.center_y, 2.);
}

#[test]
fn stationary_polyline_keeps_source_projection_fallback() {
    let path = Path::new(&[[4., -2.], [4., -2.]]).unwrap();
    let projection = path.project(9., 3.);
    assert_eq!((projection.path_s, projection.d_path), (5., 1.));
}

#[test]
fn rotation_is_removed_from_velocity_without_moving_the_point() {
    let point = Point {
        v_lead: 10.,
        d_rel: 20.,
        y_rel: 3.,
        yv_rel: -0.5,
        ..Point::default()
    };
    let velocity = velocity_in_ego_frame(&point, 0.025);
    assert_eq!(velocity, [9.925, 0.]);
    assert_eq!(point.y_rel, 3.);
}

#[test]
fn infinite_norm_takes_precedence_over_nan_coordinate() {
    let result = openpilot_radard::math::norm(&[f64::NAN, f64::INFINITY]);
    assert_eq!(result, f64::INFINITY);
}

#[test]
fn subnormal_norm_retains_the_smallest_coordinate_precision() {
    let values = [f64::from_bits(1), f64::from_bits(2), f64::from_bits(4)];
    let result = openpilot_radard::math::norm(&values);
    assert_eq!(result.to_bits(), 5);
}

#[test]
fn repeated_equal_zero_key_keeps_first_projection_until_lru_eviction() {
    let path = Path::new(&[[0., 0.], [100., 0.]]).unwrap();
    let first = path.project(25., -0.);
    let cached = Path::new(&[[0., 0.], [100., 0.]]).unwrap().project(25., 0.);
    assert_eq!(first.d_path.to_bits(), (-0_f64).to_bits());
    assert_eq!(cached.d_path.to_bits(), first.d_path.to_bits());
    for index in 0..256_u32 {
        path.project(1000. + f64::from(index), 1.);
    }
    assert_eq!(path.project(25., 0.).d_path.to_bits(), 0_f64.to_bits());
}

#[test]
fn projection_cache_hit_does_not_refresh_evicted_geometry() {
    let first = Path::new(&[[0., 0.], [100., 0.]]).unwrap();
    first.project(25., 0.);
    for index in 1..=8_u32 {
        Path::new(&[[0., 0.], [100., f64::from(index)]])
            .unwrap()
            .project(20., 1.);
    }
    let before = serde_json::to_value(openpilot_radard::path::cache::snapshot()).unwrap();
    first.project(25., 0.);
    let after = serde_json::to_value(openpilot_radard::path::cache::snapshot()).unwrap();
    assert_eq!(before["geometry"], after["geometry"]);
}
