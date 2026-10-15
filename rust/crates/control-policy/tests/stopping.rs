use openpilot_control_policy::stopping::speed;

#[test]
fn invalid_and_below_minimum_values_have_source_repairs() {
    for stored in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let value = speed(stored);
        assert_eq!(value.meters_per_second, 0.5);
        assert_eq!(value.repair, Some(50));
    }
    for stored in [-100., 0., 9.999] {
        let value = speed(stored);
        assert_eq!(value.meters_per_second, 0.1);
        assert_eq!(value.repair, Some(10));
    }
    for stored in [10., 10.25, 50., 1000.] {
        let value = speed(stored);
        assert_eq!(value.meters_per_second, stored * 0.01);
        assert_eq!(value.repair, None);
    }
}
