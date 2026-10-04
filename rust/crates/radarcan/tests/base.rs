use openpilot_radarcan::{
    base::Base,
    data::Data,
    point::{Point, Source},
    Error,
};

#[test]
fn delayed_history_uses_oldest_sample_and_preserves_empty_history_failure() {
    let mut base = Base::new(0.02, 0.05).unwrap();
    base.push_ego(10., 1.).unwrap();
    assert_eq!((base.v_ego, base.a_ego), (0., 0.));
    base.push_ego(20., 2.).unwrap();
    assert_eq!((base.v_ego, base.a_ego), (0., 0.));
    base.push_ego(30., 3.).unwrap();
    assert_eq!((base.v_ego, base.a_ego), (10., 1.));
    let mut empty = Base::new(-0.01, 0.05).unwrap();
    assert!(matches!(empty.push_ego(10., 1.), Err(Error::EmptyHistory)));
    assert_eq!(empty.v_ego, 0.);
    assert!(empty.a_ego_hist.is_empty());
    assert!(matches!(
        Base::new(-0.02, 0.05),
        Err(Error::NegativeHistory)
    ));
}

#[test]
fn flipping_owned_publication_toggles_front_only_and_leaves_raw_points_unchanged() {
    let raw = Data {
        points: vec![
            Point {
                y_rel: 1.,
                yv_rel: -0.2,
                ..Point::default()
            },
            Point {
                radar_source: Source::Corner235,
                y_rel: -2.,
                yv_rel: 0.3,
                ..Point::default()
            },
        ],
        ..Data::default()
    };
    let mut publication = raw.clone();
    publication.set_flip(true);
    assert_eq!(publication.points[0].y_rel, -1.);
    assert_eq!(publication.points[0].yv_rel, 0.2);
    assert_eq!(publication.points[1].y_rel, -2.);
    assert_eq!(raw.points[0].y_rel, 1.);
    publication.set_flip(true);
    assert_eq!(publication.points[0].y_rel, -1.);
    publication.set_flip(false);
    assert_eq!(publication.points[0].y_rel, 1.);
}
