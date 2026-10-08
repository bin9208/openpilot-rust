use openpilot_navd::geometry::{
    distance_along_geometry, limit_route_points, minimum_distance, Coordinate,
};

#[test]
fn haversine_preserves_source_radius_and_radian_order() {
    let origin = Coordinate::new(0., 0.);
    assert_eq!(
        origin.distance_to(Coordinate::new(0., 1.)).unwrap(),
        111195.05230826489
    );
    assert_eq!(
        minimum_distance(
            Coordinate::new(1., 2.),
            Coordinate::new(2., 3.),
            Coordinate::new(2., 2.)
        )
        .unwrap(),
        78608.31641207097,
    );
}

#[test]
fn short_geometry_uses_distance_from_start_even_before_segment() {
    let origin = Coordinate::new(0., 0.);
    let before = Coordinate::new(0., -0.5);
    assert_eq!(
        distance_along_geometry(&[origin, Coordinate::new(0., 1.)], before).unwrap(),
        origin.distance_to(before).unwrap(),
    );
    assert!(distance_along_geometry(&[], before).is_err());
}

#[test]
fn coincident_segment_uses_distance_from_first_point() {
    let first = Coordinate::new(37., 127.);
    let position = Coordinate::new(38., 128.);
    assert_eq!(
        minimum_distance(first, first, position).unwrap(),
        first.distance_to(position).unwrap()
    );
}

#[test]
fn route_limit_preserves_python_even_ties_and_endpoints() {
    let points = [0, 1, 2, 3, 4, 5];
    assert_eq!(limit_route_points(&points, 3).unwrap(), vec![0, 2, 5]);
    assert_eq!(limit_route_points(&points, 1).unwrap(), vec![0]);
    assert!(limit_route_points(&points, 0).unwrap().is_empty());
    assert_eq!(limit_route_points(&points, 6).unwrap(), points);
}
