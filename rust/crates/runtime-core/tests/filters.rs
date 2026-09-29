use openpilot_runtime_core::filters::{BounceFilter, FirstOrderFilter};

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-12, "{a} != {b}");
}

#[test]
fn step_response_and_rc_change() {
    let mut f = FirstOrderFilter::new(0.0, 0.1, 0.1, true);
    close(f.update(1.0), 0.5);
    close(f.update(1.0), 0.75);
    f.update_alpha(0.3);
    close(f.update(0.0), 0.5625);
    close(f.value(), 0.5625);
}

#[test]
fn cold_filter_takes_first_sample_and_zero_rc_tracks_input() {
    let mut f = FirstOrderFilter::new(100.0, 0.2, 0.1, false);
    close(f.update(-3.0), -3.0);
    f.update_alpha(0.0);
    close(f.update(9.0), 9.0);
}

#[test]
fn bounce_subthreshold_velocity_is_zeroed() {
    let mut f = BounceFilter::new(0.0, 0.1, 0.01, true, 2.0);
    close(f.update(0.00001), 0.00001 / 11.0);
    close(f.value(), 0.00001 / 11.0);
}

#[test]
fn zero_bounce_matches_first_order_over_long_sequence() {
    let mut a = FirstOrderFilter::new(2.0, 0.1, 0.01, false);
    let mut b = BounceFilter::new(2.0, 0.1, 0.01, false, 0.0);
    for i in 0..10000 {
        let x = (f64::from(i) * 0.17).sin();
        close(a.update(x), b.update(x));
    }
}
