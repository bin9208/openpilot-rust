use openpilot_radard::{
    in_lane::InLaneCornerTracker,
    model::{Model, Position, VisionLead},
    point::{Point, TrackId},
    stationary_lane::{confident_ego_lane, StoppingLaneCornerTracker},
};
fn model() -> Model {
    let xs = vec![0., 10., 20., 30., 40., 50., 60., 70., 80., 90., 100.];
    Model {
        position: Position {
            x: xs.clone(),
            y: vec![0.; xs.len()],
        },
        velocity: vec![20., 0.],
        lane_change_state: "off".into(),
        lane_lines: [-5.25, -1.75, 1.75, 5.25]
            .map(|y| Position {
                x: xs.clone(),
                y: vec![y; xs.len()],
            })
            .to_vec(),
        lane_probabilities: vec![0.9, 0.98, 0.98, 0.9],
        lane_stds: vec![0.1; 4],
        ..Model::default()
    }
}
#[test]
fn confidence_requires_current_complete_geometry_and_never_extrapolates() {
    let mut m = model();
    let (lane, _) = confident_ego_lane(&m).unwrap();
    assert_eq!(lane.offset(50., 0.), Some(0.));
    assert_eq!(lane.offset(101., 0.), None);
    assert_eq!(lane.offset(50., 2.), None);
    m.lane_change_state = "preLaneChange".into();
    assert!(confident_ego_lane(&m).is_none());
    m.lane_change_state = "off".into();
    m.lane_stds[1] = f64::NAN;
    assert!(confident_ego_lane(&m).is_none());
}
#[test]
fn stationary_corner_cannot_admit_adjacent_or_currently_ambiguous_return() {
    let mut tracker = StoppingLaneCornerTracker::default();
    let mut m = model();
    m.position.x = vec![0., 10., 20., 30., 40., 50., 60.];
    m.position.y = vec![0.; 7];
    let mut p = Point {
        source: "corner235".into(),
        track_id: TrackId(1),
        measured: true,
        d_rel: 70.,
        v_rel: -15.,
        ..Point::default()
    };
    let mut vision = VisionLead {
        probability: 0.9,
        d_rel: 70.,
        velocity: 0.,
        ..VisionLead::default()
    };
    let mut result = None;
    for i in 0..12 {
        let t = f64::from(i) * 0.05;
        p.d_rel = 70. - 15. * t;
        vision.d_rel = p.d_rel;
        result = tracker.update(t, 15., &m, std::slice::from_ref(&p), Some(vision));
    }
    assert!(result.is_some());
    let mut second = p.clone();
    second.track_id = TrackId(2);
    assert!(tracker
        .update(0.6, 15., &m, &[p.clone(), second], Some(vision))
        .is_none());
    p.y_rel = 3.;
    assert!(tracker.update(0.65, 15., &m, &[p], Some(vision)).is_none());
}
#[test]
fn repeating_measurement_never_acquires_in_lane_braking_lead() {
    let mut tracker = InLaneCornerTracker::default();
    let mut m = model();
    m.velocity = vec![20., 10.];
    let p = Point {
        source: "corner235".into(),
        track_id: TrackId(1),
        measured: true,
        d_rel: 70.,
        v_rel: -10.,
        v_lead: 10.,
        a_lead: -2.,
        ..Point::default()
    };
    let vision = VisionLead {
        probability: 0.9,
        d_rel: 70.,
        velocity: 10.,
        x_std: 1.,
        y_std: 1.,
        v_std: 1.,
        ..VisionLead::default()
    };
    for i in 0..20 {
        assert!(tracker
            .update(
                f64::from(i) * 0.05,
                20.,
                &m,
                std::slice::from_ref(&p),
                Some(vision),
                Some(0.)
            )
            .is_none());
    }
}
