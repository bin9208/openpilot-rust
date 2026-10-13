use openpilot_cluster::route_replay::corner::{decode_raw, RawObject, StableTracker};

#[test]
fn raw_and_stable_corner_display_preserves_side_objects_hit_gate_and_expiry() {
    let mut object = RawObject {
        group: "235".into(),
        address: 0x235,
        quality: 20,
        age: 1,
        object_id: 3,
        x: 20.0,
        y: 2.0,
        vx: -1.0,
        ..Default::default()
    };
    assert!(object.valid());
    let mut tracker = StableTracker::default();
    for index in 0..4 {
        object.t = f64::from(index) * 0.05;
        object.x = 20.0 - object.t;
        tracker.update(&object).unwrap();
        let points = tracker.points_at(object.t, 72.0);
        assert_eq!(points.len(), usize::from(index == 3));
    }
    let points = tracker.points_at(object.t, 72.0);
    assert_eq!(points[0].label, "CO003");
    assert_eq!(points[0].valid_count, Some(4));
    assert!(points[0].lateral_m < 0.0);
    assert!(tracker.points_at(object.t + 0.551, 72.0).is_empty());
    object.x = 0.0;
    assert!(object.valid());
    object.y = 0.5;
    assert!(!object.valid());
    assert!(decode_raw(1.0, 0x235, &[0; 31]).is_empty());
    assert_eq!(decode_raw(1.0, 0x180, &[0; 32]).len(), 2);
}
