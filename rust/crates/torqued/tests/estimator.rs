use openpilot_torqued::{
    buckets::Buckets,
    estimator::{Car, Estimator, Identity},
    history::{History, Input},
    random::RandomState,
    Error, Fit,
};
struct Fixed([f64; 3]);
impl Fit for Fixed {
    fn estimate(&mut self, _: &[[f64; 3]]) -> Result<[f64; 3], Error> {
        Ok(self.0)
    }
}
fn estimator() -> Estimator {
    Estimator::new(
        Car {
            identity: Identity {
                fingerprint: "test".to_owned(),
                tuning: 3,
                torque: Some([0.12, 2.]),
            },
            allowed_brand: true,
        },
        (true, true),
        RandomState::seeded(42),
    )
}
fn ready(estimator: &mut Estimator) {
    for x in [-0.4, -0.25, -0.15, -0.05, 0.05, 0.15, 0.25, 0.4] {
        for _ in 0..75 {
            estimator.buckets.add(x, 2. * x);
        }
    }
}
#[test]
fn random_state_matches_source_permutation_when_seeded() {
    // Given the original np.random.seed(42) sequence.
    let mut random = RandomState::seeded(42);
    // When choice samples all entries, it still permutes the population.
    let sample = random.sample_indices(10, 10).unwrap();
    // Then every selected index matches the source's MT19937/rejection shuffle.
    assert_eq!(sample, [8, 1, 5, 0, 7, 2, 9, 4, 3, 6]);
}
#[test]
fn buckets_retain_fifo_when_capacity_is_reached() {
    // Given an overflowing bucket and points on excluded boundaries.
    let mut buckets = Buckets::new(false);
    for y in 0..1505 {
        buckets.add(-0.5, f64::from(y));
    }
    buckets.add(0.5, 1.);
    buckets.add(f64::NAN, 1.);
    // When observing the ordered point population.
    let points = buckets.points();
    // Then only the last 1500 in-range points remain in insertion order.
    assert_eq!(points.len(), 1500);
    assert_eq!(points[0], [-0.5, 1., 5.]);
    assert_eq!(points[1499], [-0.5, 1., 1504.]);
}
#[test]
fn filters_use_old_alpha_then_new_decay_when_valid() {
    // Given valid decimated buckets and offline parameters.
    let mut estimator = estimator();
    ready(&mut estimator);
    // When raw values exceed both sanity clips.
    let packet = estimator.message(&mut Fixed([9., 0.3, 5.]), true).unwrap();
    // Then updates use the original 50-second alpha before advancing decay.
    let alpha = 0.05 / 50.05;
    assert!(packet.live_valid);
    assert_eq!(packet.decay, 50.05);
    assert_eq!(
        packet.filtered,
        [
            (1. - alpha) * 2. + alpha * 3.,
            alpha * 0.3,
            (1. - alpha) * 0.12 + alpha * (1.8 * 0.12)
        ]
    );
}
#[test]
fn nan_fit_resets_points_but_preserves_filters_and_lag_when_valid() {
    // Given a previously fitted estimator with accumulated raw history and delay.
    let mut estimator = estimator();
    ready(&mut estimator);
    let previous = estimator
        .message(&mut Fixed([2.1, 0.3, 0.13]), false)
        .unwrap()
        .filtered;
    estimator.history.handle(Input::Delay(0.3)).unwrap();
    // When the original SVD path returns invalid numerical parameters.
    let packet = estimator.message(&mut Fixed([f64::NAN; 3]), true).unwrap();
    // Then reset preserves filters/calibration/lag but clears bucket points and decay.
    assert!(!packet.live_valid);
    assert_eq!(packet.count, 0);
    assert_eq!(packet.resets, 2.);
    assert_eq!(packet.decay, 50.);
    assert_eq!(packet.filtered, previous);
    assert_eq!(estimator.history.lag, 0.3);
}
#[test]
fn history_reports_empty_interpolation_when_only_output_is_warm() {
    // Given 100 output messages and absent control/state samples.
    let mut history = History::default();
    for i in 0..100 {
        history
            .handle(Input::Output {
                time: f64::from(i) * 0.05,
                torque: 0.2,
            })
            .unwrap();
    }
    // When valid livePose triggers the same source interpolation.
    let result = history.handle(Input::Pose {
        time: 5.,
        roll: 0.,
        angular: [0., 0., 0.03],
        valid: true,
    });
    // Then the missing input history is an observable error, not a fabricated sample.
    assert!(result.is_err());
}
