use openpilot_radarcan::{
    lead_filter::LeadFilter,
    scalar::{float_sum, maximum, minimum, square},
    Error,
};

#[test]
fn invalid_period_is_a_constructor_failure() {
    for dt in [0., -0.05, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            LeadFilter::new(10., dt),
            Err(Error::InvalidPeriod)
        ));
    }
}

#[test]
fn ordered_min_max_keep_first_argument_for_nan_and_equal_signed_zero() {
    assert_eq!(minimum(0.5, f64::NAN), 0.5);
    assert_eq!(maximum(-0.5, f64::NAN), -0.5);
    assert!(minimum(f64::NAN, 0.5).is_nan());
    assert_eq!(minimum(-0., 0.).to_bits(), (-0_f64).to_bits());
    assert_eq!(maximum(-0., 0.).to_bits(), (-0_f64).to_bits());
}

#[test]
fn python312_sum_preserves_small_terms_after_cancellation() {
    assert_eq!(float_sum([1e16, 1., -1e16]), 1.);
    assert_eq!(float_sum([f64::INFINITY, 1.]), f64::INFINITY);
    assert!(float_sum([f64::INFINITY, f64::NEG_INFINITY]).is_nan());
}

#[test]
fn source_power_retains_libm_rounding_and_python_overflow() {
    let ratio = f64::from_bits(0x3e79_5209_2748_c0ba);
    assert_eq!(square(ratio).unwrap().to_bits(), 0x3d04_0900_9c4d_a048);
    assert_ne!((ratio * ratio).to_bits(), 0x3d04_0900_9c4d_a048);
    assert!(matches!(square(1e200), Err(Error::PowerOverflow)));
    assert_eq!(square(f64::INFINITY).unwrap(), f64::INFINITY);
}
