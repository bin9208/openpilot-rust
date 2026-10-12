use openpilot_cluster::scene::{lane_centerline, strip_from_centerline, Vec3};

#[test]
fn centerline_strip_preserves_source_right_vector_orientation() {
    let points = lane_centerline(0.0, 0.0, 3.6, -2.0, 8.0, 2, 0.1);
    assert_eq!(
        points,
        vec![
            Vec3::new(0.0, -2.0, 0.1),
            Vec3::new(0.0, 3.0, 0.1),
            Vec3::new(0.0, 8.0, 0.1)
        ]
    );
    let strip = strip_from_centerline(&points, 1.8, [255, 255, 255, 242]);
    assert_eq!(strip.left[0], Vec3::new(-0.9, -2.0, 0.1));
    assert_eq!(strip.right[2], Vec3::new(0.9, 8.0, 0.1));
}
